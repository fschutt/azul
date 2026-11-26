//! The fake AzReview: a workspace to browse, a viewport that follows the phone, a pen, the four buttons and the
//! conversations a press starts. The simulator and the phone's tests lean on it, so it is held to account.

use super::*;
use azul_review_link::frame::PairReject;
use azul_review_link::limits::{MAX_BUTTON_TEXT_CHARS, MAX_THUMBNAIL_BYTES};
use azul_review_link::viewport::{apply_gesture, fit_width, resolve_focus};

fn names(rows: &Rows) -> Vec<&str> {
    rows.entries.iter().map(|e| e.name.as_str()).collect()
}

fn gesture(id: u32, phase: Phase, pan_x: f32, pan_y: f32, zoom: f32) -> ViewportOp {
    ViewportOp::Gesture { id, phase, pan_x, pan_y, zoom, focus: Focus::Pen }
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

// ---- the workspace

#[test]
fn the_workspace_lists_its_folders_first_and_then_its_files() {
    let mac = SimMac::new();
    let root = mac.list(1, "", 0);
    assert_eq!(root.dir, "");
    assert_eq!(root.request_id, 1);
    assert!(!root.more);
    let first_file = root.entries.iter().position(|e| e.kind == EntryKind::File).expect("a file in the root");
    assert!(first_file > 0 && root.entries[..first_file].iter().all(|e| e.kind == EntryKind::Dir), "{:?}", names(&root));
    assert!(root.entries[first_file..].iter().all(|e| e.kind == EntryKind::File));
    assert!(names(&root).contains(&"layout") && names(&root).contains(&"README.md"));
}

#[test]
fn a_folder_lists_what_is_in_it_and_an_unknown_one_lists_nothing() {
    let mac = SimMac::new();
    let src = mac.list(2, "layout/src", 0);
    assert!(names(&src).contains(&"solver3") && names(&src).contains(&"window.rs"), "{:?}", names(&src));
    let empty = mac.list(3, "no/such/folder", 0);
    assert!(empty.entries.is_empty() && !empty.more);
    let sneaky = mac.list(4, "../../etc", 0);
    assert!(sneaky.entries.is_empty(), "the workspace is all there is");
}

#[test]
fn a_very_long_folder_comes_in_pages_of_at_most_the_rows_a_frame_may_hold() {
    let mac = SimMac::new();
    let first = mac.list(1, "tests/fixtures", 0);
    assert_eq!(first.entries.len(), MAX_ROWS);
    assert!(first.more);
    let second = mac.list(2, "tests/fixtures", MAX_ROWS as u32);
    assert!(!second.more && !second.entries.is_empty() && second.entries.len() < MAX_ROWS);
    assert_eq!(second.offset, MAX_ROWS as u32);
    let past = mac.list(3, "tests/fixtures", 100_000);
    assert!(past.entries.is_empty() && !past.more);
}

#[test]
fn the_files_carry_the_colours_azreview_gives_them_and_a_folder_carries_the_worst_of_what_is_in_it() {
    let mac = SimMac::new();
    let all = ["", "layout", "layout/src", "layout/src/solver3", "core/src", "examples/azul-review/src"];
    let mut seen = std::collections::BTreeSet::new();
    for dir in all {
        for e in mac.list(1, dir, 0).entries {
            seen.insert(format!("{:?}", e.state));
        }
    }
    for wanted in ["None", "Clean", "Unresolved", "Working"] {
        assert!(seen.contains(wanted), "no {wanted} in the fixture: {seen:?}");
    }
    let folder_state = mac.state_of("layout");
    let worst = mac
        .list(1, "layout", 0)
        .entries
        .iter()
        .map(|e| e.state)
        .fold(FileState::None, |a, b| worse(a, b));
    assert_eq!(folder_state, worst, "a folder is as bad as its worst child");
}

fn worse(a: FileState, b: FileState) -> FileState {
    let rank = |s: FileState| match s {
        FileState::None => 0,
        FileState::Clean => 1,
        FileState::Unresolved => 2,
        FileState::Working => 3,
    };
    if rank(b) > rank(a) {
        b
    } else {
        a
    }
}

// ---- opening a file

#[test]
fn opening_a_file_shows_it_from_the_top_with_its_pages_and_a_new_picture() {
    let mut mac = SimMac::new();
    let before = mac.pen_state();
    mac.open_file(100, "core/src/dom.rs", Some(77));
    let after = mac.pen_state();
    assert_eq!(after.file, "core/src/dom.rs");
    assert_eq!(after.line, 77);
    assert_eq!(after.page_index, 0);
    assert!(after.page_count >= 3 && after.page_count <= 12, "{}", after.page_count);
    assert!(after.thumb_id != before.thumb_id, "another page, another picture");
    assert!(near(after.view.x, 0.0) && near(after.view.y, 0.0), "from the top: {:?}", after.view);
    assert!(mac.log.last().is_some_and(|l| l.text.contains("core/src/dom.rs")), "{:?}", mac.log.last());
}

#[test]
fn opening_without_a_line_goes_to_the_first_unreviewed_line_and_the_pages_depend_on_the_file_only() {
    let mut a = SimMac::new();
    let mut b = SimMac::new();
    a.open_file(0, "layout/src/window.rs", None);
    b.open_file(0, "layout/src/window.rs", None);
    assert!(a.line >= 1);
    assert_eq!((a.page_count, a.line), (b.page_count, b.line), "the same file is the same file");
    b.open_file(1, "core/src/events.rs", None);
    assert_ne!(a.file, b.file);
}

#[test]
fn a_file_that_is_not_in_the_workspace_is_not_opened() {
    let mut mac = SimMac::new();
    let before = mac.file.clone();
    mac.open_file(0, "../../etc/passwd", None);
    mac.open_file(0, "/etc/passwd", None);
    assert_eq!(mac.file, before);
    assert!(mac.log.iter().any(|l| l.text.contains("refused")), "{:?}", mac.log);
}

// ---- the viewport follows the phone

#[test]
fn a_gesture_moves_the_view_from_where_it_began_by_the_cumulative_totals() {
    let mut mac = SimMac::new();
    let base = mac.view;
    mac.apply(0, gesture(1, Phase::Begin, 0.0, 0.0, 1.0));
    mac.apply(10, gesture(1, Phase::Update, 0.05, 0.1, 1.0));
    let about = resolve_focus(Focus::Pen, base, mac.pen.map(|p| (p.x, p.y)));
    let expected = apply_gesture(base, (0.05, 0.1), 1.0, about);
    assert!(near(mac.view.x, expected.x) && near(mac.view.y, expected.y), "{:?} {expected:?}", mac.view);
    // the same totals again change nothing: a duplicate or a late frame is harmless
    let seen = mac.view;
    mac.apply(11, gesture(1, Phase::Update, 0.05, 0.1, 1.0));
    assert_eq!(mac.view, seen);
    // more totals move it on from the base, not from where it is
    mac.apply(20, gesture(1, Phase::Update, 0.1, 0.1, 1.0));
    let expected = apply_gesture(base, (0.1, 0.1), 1.0, about);
    assert!(near(mac.view.x, expected.x), "{:?} {expected:?}", mac.view);
    mac.apply(30, gesture(1, Phase::End, 0.1, 0.1, 1.0));
    assert!(near(mac.view.x, expected.x));
}

#[test]
fn a_pinch_zooms_about_the_pen_and_the_pen_stays_where_it_is_on_the_screen() {
    let mut mac = SimMac::new();
    mac.pen = Some(PenMark { x: 0.5, y: 0.5, down: false });
    mac.view = Rect { x: 0.25, y: 0.25, w: 0.5, h: 0.5 };
    let (rx, ry) = ((0.5 - mac.view.x) / mac.view.w, (0.5 - mac.view.y) / mac.view.h);
    mac.apply(0, gesture(1, Phase::Begin, 0.0, 0.0, 1.0));
    mac.apply(1, gesture(1, Phase::Update, 0.0, 0.0, 2.0));
    assert!(near(mac.view.w, 0.25), "{:?}", mac.view);
    let (nx, ny) = ((0.5 - mac.view.x) / mac.view.w, (0.5 - mac.view.y) / mac.view.h);
    assert!(near(rx, nx) && near(ry, ny), "the pen keeps its place on the screen");
}

#[test]
fn an_update_that_comes_before_its_begin_still_works_and_a_gesture_that_never_ends_is_replaced() {
    let mut mac = SimMac::new();
    let base = mac.view;
    mac.apply(0, gesture(7, Phase::Update, 0.0, 0.1, 1.0));
    assert!(mac.view.y > base.y, "the first sight of a gesture is its start");
    mac.apply(1, gesture(8, Phase::Begin, 0.0, 0.0, 1.0));
    let before = mac.view;
    mac.apply(2, gesture(8, Phase::Update, 0.1, 0.0, 1.0));
    assert!(mac.view.x > before.x, "a new gesture takes over from where the view is");
}

#[test]
fn fit_makes_the_width_the_page_and_reset_goes_to_the_top() {
    let mut mac = SimMac::new();
    mac.view = Rect { x: 0.3, y: 0.4, w: 0.3, h: 0.24 };
    mac.apply(0, ViewportOp::Fit);
    assert!(near(mac.view.w, 1.0) && near(mac.view.x, 0.0), "{:?}", mac.view);
    mac.view = Rect { x: 0.3, y: 0.4, w: 0.3, h: 0.24 };
    mac.apply(1, ViewportOp::Reset);
    assert!(near(mac.view.w, 1.0) && near(mac.view.y, 0.0), "{:?}", mac.view);
    assert!(near(fit_width(Rect { x: 0.0, y: 0.0, w: 0.5, h: 0.4 }).w, 1.0));
}

#[test]
fn a_page_step_turns_the_page_within_the_file_and_gets_a_new_picture() {
    let mut mac = SimMac::new();
    mac.open_file(0, "layout/src/window.rs", None);
    let pages = mac.page_count;
    let id = mac.pen_state().thumb_id;
    mac.apply(1, ViewportOp::PageStep(1));
    assert_eq!(mac.page, 1);
    assert_ne!(mac.pen_state().thumb_id, id);
    mac.apply(2, ViewportOp::PageStep(-1));
    mac.apply(3, ViewportOp::PageStep(-1));
    assert_eq!(mac.page, 0, "not before the first page");
    for _ in 0..(pages + 5) {
        mac.apply(4, ViewportOp::PageStep(1));
    }
    assert_eq!(mac.page, pages - 1, "not past the last");
    mac.apply(5, ViewportOp::PageStep(0));
    assert_eq!(mac.page, pages - 1);
}

// ---- the context the buttons act on

#[test]
fn the_buttons_act_on_the_line_under_the_pen_or_else_on_the_page() {
    let mut mac = SimMac::new();
    mac.pen = Some(PenMark { x: 0.4, y: 0.4, down: false });
    assert_eq!(mac.context(), Context::Line);
    assert_eq!(mac.pen_state().context, Context::Line);
    mac.pen = None;
    assert_eq!(mac.context(), Context::Page);
    mac.file.clear();
    assert_eq!(mac.pen_state().line, 0, "no file, no line");
}

// ---- the buttons and their conversations

#[test]
fn a_press_starts_a_conversation_that_goes_received_working_suggested_done_each_once() {
    let mut mac = SimMac::new();
    mac.press_button(1000, 1, 77);
    let replies = |frames: &[Frame]| -> Vec<(u32, ReplyStatus)> {
        frames
            .iter()
            .filter_map(|f| match f {
                Frame::Reply(r) => Some((r.request_id, r.status)),
                _ => None,
            })
            .collect()
    };
    assert_eq!(replies(&mac.tick(1000)), [(77, ReplyStatus::Received)]);
    assert!(replies(&mac.tick(1100)).is_empty());
    assert_eq!(replies(&mac.tick(1400)), [(77, ReplyStatus::Working)]);
    assert!(replies(&mac.tick(3000)).is_empty());
    assert_eq!(replies(&mac.tick(5200)), [(77, ReplyStatus::Suggested)]);
    assert_eq!(replies(&mac.tick(10_300)), [(77, ReplyStatus::Done)]);
    assert!(replies(&mac.tick(20_000)).is_empty(), "it is over");
}

#[test]
fn the_text_of_the_button_is_what_the_conversation_is_about_and_the_tree_shows_it_working() {
    let mut mac = SimMac::new();
    mac.set_buttons(0, &ButtonSet::defaults());
    let file = mac.file.clone();
    mac.press_button(0, 0, 5);
    assert!(mac.log.iter().any(|l| l.text.contains("Tone") && l.text.contains("Slop wording")), "{:?}", mac.log);
    mac.tick(400);
    assert_eq!(mac.state_of(&file), FileState::Working);
    mac.tick(5000);
    assert_eq!(mac.state_of(&file), FileState::Unresolved);
    mac.tick(11_000);
    assert_eq!(mac.state_of(&file), FileState::Clean);
}

#[test]
fn two_buttons_pressed_together_are_two_conversations() {
    let mut mac = SimMac::new();
    mac.press_button(0, 0, 1);
    mac.press_button(500, 2, 2);
    let frames = mac.tick(5200);
    let statuses: Vec<(u32, ReplyStatus)> = frames
        .iter()
        .filter_map(|f| if let Frame::Reply(r) = f { Some((r.request_id, r.status)) } else { None })
        .collect();
    assert!(statuses.contains(&(1, ReplyStatus::Suggested)), "{statuses:?}");
    assert!(statuses.contains(&(2, ReplyStatus::Working)), "{statuses:?}");
    assert_eq!(mac.conversations.len(), 2);
}

#[test]
fn a_button_that_does_not_exist_starts_nothing() {
    let mut mac = SimMac::new();
    mac.press_button(0, 4, 9);
    mac.press_button(0, usize::MAX, 9);
    assert!(mac.conversations.is_empty());
    assert!(mac.log.iter().any(|l| l.text.contains("no such button")), "{:?}", mac.log);
}

#[test]
fn saved_buttons_are_cut_to_their_caps_and_kept() {
    let mut mac = SimMac::new();
    let mut set = ButtonSet::defaults();
    set.buttons[0].label = "A far too long label".into();
    set.buttons[1].text = "x".repeat(MAX_BUTTON_TEXT_CHARS + 20);
    let cut = mac.set_buttons(0, &set);
    assert_eq!(cut.len(), 2, "{cut:?}");
    assert_eq!(mac.buttons.buttons[0].label.chars().count(), 12);
    assert_eq!(mac.buttons.buttons[1].text.chars().count(), MAX_BUTTON_TEXT_CHARS);
    assert!(mac.log.iter().any(|l| l.text.contains("buttons")), "{:?}", mac.log);
}

// ---- time: the pen and the picture

#[test]
fn the_first_tick_sends_the_picture_once_and_a_pen_state() {
    let mut mac = SimMac::new();
    let frames = mac.tick(0);
    assert!(frames.iter().any(|f| matches!(f, Frame::Thumbnail(_))));
    assert!(frames.iter().any(|f| matches!(f, Frame::PenState(_))));
    let again = mac.tick(100);
    assert!(!again.iter().any(|f| matches!(f, Frame::Thumbnail(_))), "the picture goes once");
    assert!(!again.iter().any(|f| matches!(f, Frame::PenState(_))), "nothing changed, so no state");
}

#[test]
fn a_change_of_page_sends_the_new_picture_and_a_state_that_names_it() {
    let mut mac = SimMac::new();
    mac.tick(0);
    mac.apply(50, ViewportOp::PageStep(1));
    let frames = mac.tick(100);
    let thumb = frames.iter().find_map(|f| if let Frame::Thumbnail(t) = f { Some(t.id) } else { None }).expect("a new picture");
    let state = frames.iter().find_map(|f| if let Frame::PenState(p) = f { Some(p.thumb_id) } else { None }).expect("a new state");
    assert_eq!(thumb, state);
}

#[test]
fn a_walking_pen_sends_at_most_thirty_states_a_second_and_stays_on_the_page() {
    let mut mac = SimMac::new();
    mac.pen_walk = true;
    let mut states = 0;
    for ms in 0..2000u64 {
        for f in mac.tick(ms) {
            if let Frame::PenState(p) = f {
                states += 1;
                if let Some(pen) = p.pen {
                    assert!((0.0..=1.0).contains(&pen.x) && (0.0..=1.0).contains(&pen.y), "{pen:?}");
                }
            }
        }
    }
    assert!((40..=60).contains(&states), "{states} states in two seconds");
}

#[test]
fn a_pen_that_stands_still_sends_nothing_after_the_first_state() {
    let mut mac = SimMac::new();
    mac.pen_walk = false;
    let mut states = 0;
    for ms in (0..2000u64).step_by(7) {
        states += mac.tick(ms).iter().filter(|f| matches!(f, Frame::PenState(_))).count();
    }
    assert_eq!(states, 1);
}

// ---- the picture of a page

#[test]
fn the_picture_of_a_page_is_raw_pixels_that_fit_a_frame_and_look_like_a_page_of_code() {
    let mac = SimMac::new();
    let thumb = mac.thumbnail();
    assert_eq!((thumb.width, thumb.height), (THUMB_W, THUMB_H));
    assert_eq!(thumb.format, ThumbFormat::Rgba8);
    assert_eq!(thumb.bytes.len(), usize::from(THUMB_W) * usize::from(THUMB_H) * 4);
    assert!(thumb.bytes.len() <= MAX_THUMBNAIL_BYTES);
    let pixels = thumb.bytes.chunks(4);
    let dark = pixels.clone().filter(|p| p[0] < 120 && p[1] < 120 && p[2] < 120).count();
    let light = pixels.filter(|p| p[0] > 230 && p[1] > 225 && p[2] > 215).count();
    let total = usize::from(THUMB_W) * usize::from(THUMB_H);
    assert!(light * 100 > total * 60, "mostly paper: {light} of {total}");
    assert!(dark * 100 > total * 3 && dark * 100 < total * 40, "some text: {dark} of {total}");
    assert!(thumb.bytes.chunks(4).all(|p| p[3] == 255), "opaque");
}

#[test]
fn every_page_has_its_own_picture_and_the_same_page_always_the_same() {
    let mut mac = SimMac::new();
    let first = mac.thumbnail();
    assert_eq!(first, mac.thumbnail());
    mac.apply(0, ViewportOp::PageStep(1));
    assert_ne!(first.bytes, mac.thumbnail().bytes);
    mac.apply(1, ViewportOp::PageStep(-1));
    assert_eq!(first.bytes, mac.thumbnail().bytes);
}

// ---- the log

#[test]
fn the_log_keeps_the_newest_lines_and_the_time_of_each() {
    let mut mac = SimMac::new();
    for i in 0..(LOG_LINES as u64 + 50) {
        mac.say(i, format!("line {i}"));
    }
    assert_eq!(mac.log.len(), LOG_LINES);
    assert_eq!(mac.log.last().map(|l| l.at), Some(LOG_LINES as u64 + 49));
    assert_eq!(mac.log.first().map(|l| l.text.as_str()), Some("line 50"));
    let _ = PairReject::Busy;
}
