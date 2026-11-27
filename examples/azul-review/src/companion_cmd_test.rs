//! What the phone's commands and the phone's state make of the app's state, without a window: the geometry the zoom model is given, where the
//! strip is, where the pen is, and the pen state the phone is told.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use azul_review_link::frame::Context as PenContext;

use super::*;
use crate::{lib_test::app_with_file, page_view::Z_MAX};

fn strip() -> ui::StripNow {
    ui::StripNow { scroll: (0.0, 0.0), size: (1100.0, 700.0) }
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.01 * b.abs().max(1.0)
}

#[test]
fn the_window_of_an_app_is_its_pages_the_strip_and_the_size_of_what_it_shows() {
    let s = app_with_file();
    let (geometry, looking, pen) = window_of(&s, &strip()).expect("a file is open");
    assert_eq!(geometry.pages, 3, "120 lines of 46 to a page");
    assert_eq!(geometry.window, (1100.0, 700.0));
    assert_eq!((geometry.page_w, geometry.gap), (ui::PAGE_W, ui::PAGE_GAP));
    assert_eq!(looking.page, 0);
    assert!(near(looking.zoom, 1.0) && near(looking.at.0, 0.0) && near(looking.at.1, 0.0), "{looking:?}");
    assert_eq!(pen, None, "the pen has not been seen");
}

#[test]
fn where_the_strip_is_scrolled_is_the_page_and_the_place_on_it_at_the_zoom_the_pages_are_drawn_at() {
    let mut s = app_with_file();
    let scrolled = ui::StripNow { scroll: (2.0 * ui::page_stride(), 0.0), size: (1100.0, 700.0) };
    assert_eq!(window_of(&s, &scrolled).map(|w| w.1.page), Some(2));
    // zoomed in the strip scrolls in the px of the screen: a page is that much longer
    s.zoom = 1.5;
    let zoomed = ui::StripNow { scroll: (ui::page_stride() * 1.5, 150.0), size: (1100.0, 700.0) };
    let looking = window_of(&s, &zoomed).expect("a file").1;
    assert_eq!(looking.page, 1);
    assert!(near(looking.zoom, 1.5) && near(looking.at.1, 100.0), "{looking:?}");
    s.zoom = 50.0;
    assert!(near(window_of(&s, &strip()).expect("a file").1.zoom, Z_MAX), "a zoom past the limit is held");
}

#[test]
fn there_is_no_window_without_a_file() {
    let s = AppState::create(PathBuf::from("/work/proj"), Vec::new(), RefAny::new(0u8), PathBuf::new());
    assert!(window_of(&s, &strip()).is_none());
}

#[test]
fn the_pen_is_in_the_zoom_models_units_while_it_was_seen_lately_and_is_away_after_a_while() {
    let mut s = app_with_file();
    s.pen_seen = Some((PenAt { page: 1, x: 500.0, y: 200.0, down: true }, Instant::now()));
    let pen = window_of(&s, &strip()).expect("a file").2.expect("the pen is here");
    assert!(near(pen.0, 0.5), "{pen:?}");
    s.pen_seen = Some((PenAt { page: 1, x: 500.0, y: 200.0, down: true }, Instant::now() - Duration::from_millis(companion_pen::PEN_AWAY_MS + 500)));
    assert_eq!(window_of(&s, &strip()).expect("a file").2, None, "away");
}

#[test]
fn the_phone_is_told_the_folder_the_file_the_pages_and_the_window() {
    let s = AppState::create(PathBuf::from("/work/proj"), vec![crate::lib_test::file_named("src/lib.rs")], RefAny::new(0u8), PathBuf::new());
    let state = pen_state_of(&s, Some(&strip()), None);
    assert_eq!(state.folder, "proj");
    let page = state.page().expect("the pages");
    assert_eq!((page.file.as_str(), page.page_count, page.line_count), ("src/lib.rs", 3, 120));
    assert!(near(state.view.w, 1.1) && near(state.view.h, 0.7), "{:?}", state.view);
    assert_eq!(state.context, PenContext::Page, "the pen has not been seen: a button acts on the page");
    assert_eq!(state.pen, None);
    assert_eq!(state.return_point, None);
}

#[test]
fn a_pen_over_a_line_makes_a_button_act_on_the_line_and_the_line_is_the_pages_first_plus_the_rows_down_to_the_pen() {
    let mut s = app_with_file();
    let y = ui::PAGE_PAD + 3.0 * 15.0;
    s.pen_seen = Some((PenAt { page: 1, x: 200.0, y, down: false }, Instant::now()));
    let state = pen_state_of(&s, Some(&strip()), None);
    assert_eq!(state.context, PenContext::Line);
    let page = state.page().expect("the pages");
    assert_eq!(page.line as usize, crate::code::SourceFile::first_line_of(1) + ui::line_of(y));
    assert!(state.pen.is_some_and(|p| !p.down));
}

#[test]
fn a_file_with_no_room_for_the_pen_line_has_no_line() {
    let mut s = app_with_file();
    // below the last line of the file: the page's rows run out before the pen's height does
    s.pen_seen = Some((PenAt { page: 2, x: 200.0, y: ui::PAGE_PAD + 40.0 * 15.0, down: false }, Instant::now()));
    let line = pen_state_of(&s, Some(&strip()), None).page().map(|p| p.line);
    assert_eq!(line, Some(0), "line 91 + 40 is past line 120");
}

#[test]
fn the_state_has_the_zoom_the_pages_are_drawn_at_and_the_place_to_go_back_to() {
    let mut s = app_with_file();
    s.zoom = 2.0;
    let zoomed = ui::StripNow { scroll: (0.0, 0.0), size: (1100.0, 700.0) };
    let state = pen_state_of(&s, Some(&zoomed), None);
    assert!(near(state.view.w, 0.55), "{:?}", state.view);
    let _ = s.progress.jumped_from(crate::progress::Back { file: "f.rs".to_string(), page: 1, line: 50 });
    let back = pen_state_of(&s, Some(&zoomed), None).return_point.expect("the way back");
    assert_eq!((back.file.as_str(), back.page_index, back.line), ("f.rs", 1, 50));
    assert!(back.label.contains("f.rs"), "{back:?}");
}

#[test]
fn with_no_file_or_no_strip_the_phone_is_told_the_folder_and_an_empty_page() {
    let none = AppState::create(PathBuf::from("/work/proj"), Vec::new(), RefAny::new(0u8), PathBuf::new());
    let state = pen_state_of(&none, Some(&strip()), None);
    assert_eq!(state.folder, "proj");
    assert_eq!(state.page().map(|p| p.file.as_str()), Some(""));
    let unlaid = pen_state_of(&app_with_file(), None, None);
    assert_eq!(unlaid.page().map(|p| p.file.as_str()), Some(""), "the pages are not laid out yet");
}

#[test]
fn two_scroll_offsets_differ_by_more_than_a_tenth_of_a_pixel() {
    assert!(!scroll_differs((10.0, 20.0), (10.05, 19.95)));
    assert!(scroll_differs((10.0, 20.0), (10.2, 20.0)));
    assert!(scroll_differs((10.0, 20.0), (10.0, 19.8)));
}

// ---- the debug server's op

#[test]
fn the_ops_arguments_are_the_command_they_stand_for() {
    assert_eq!(parse_op(r#"{"op": "fit"}"#), Ok(ViewportOp::Fit));
    assert_eq!(parse_op(r#"{"op": "reset"}"#), Ok(ViewportOp::Reset));
    assert_eq!(parse_op(r#"{"op": "page_step"}"#), Ok(ViewportOp::PageStep(1)), "a key is a page forward unless it says more");
    assert_eq!(parse_op(r#"{"op": "page_step", "steps": -2}"#), Ok(ViewportOp::PageStep(-2)));
    assert_eq!(parse_op(r#"{"op": "page_step", "steps": 900}"#), Ok(ViewportOp::PageStep(8)), "held to the most a command may ask for");
    let gesture = parse_op(r#"{"op": "gesture", "id": 7, "phase": "begin", "pan_y": 0.25, "zoom": 1.5, "focus": "center"}"#);
    assert_eq!(gesture, Ok(ViewportOp::Gesture { id: 7, phase: Phase::Begin, pan_x: 0.0, pan_y: 0.25, zoom: 1.5, focus: Focus::Center }));
    let plain = parse_op(r#"{"op": "gesture"}"#);
    assert_eq!(plain, Ok(ViewportOp::Gesture { id: 1, phase: Phase::Update, pan_x: 0.0, pan_y: 0.0, zoom: 1.0, focus: Focus::Pen }));
}

#[test]
fn an_op_that_is_none_of_the_four_or_no_json_is_a_sentence_for_the_script() {
    assert!(parse_op("{").unwrap_err().contains("no JSON"));
    assert!(parse_op(r#"{"op": "teleport"}"#).unwrap_err().contains("one of gesture, fit, page_step, reset"));
    assert!(parse_op("{}").is_err());
}

// ---- the quick buttons

#[test]
fn a_buttons_conversation_is_anchored_at_the_region_else_the_pen_else_the_head_of_the_page_in_view() {
    let y_of_pen = ui::PAGE_PAD + 200.0;
    assert_eq!(anchor_of(Some((2, 300.0)), Some((1, y_of_pen)), 0, 3), (2, 300.0), "a loop that waits is what a button acts on first");
    assert_eq!(anchor_of(None, Some((1, y_of_pen)), 0, 3), (1, y_of_pen), "then the line under the pen");
    assert_eq!(anchor_of(None, None, 2, 3), (2, ui::PAGE_PAD), "then the page");
}

#[test]
fn an_anchor_is_held_to_the_file_and_to_the_page() {
    assert_eq!(anchor_of(None, None, 99, 3).0, 2, "a page past the last is the last");
    assert_eq!(anchor_of(None, Some((0, -50.0)), 0, 3).1, ui::PAGE_PAD, "above the page's padding: at it");
    assert_eq!(anchor_of(None, Some((0, 1.0e9)), 0, 3).1, ui::page_h() - ui::PAGE_PAD, "below the page: at its foot");
    assert_eq!(anchor_of(None, Some((0, f32::NAN)), 0, 3).1, ui::PAGE_PAD, "a height that is no number");
    assert_eq!(anchor_of(None, None, 0, 0).0, 0, "a file of no pages");
}

// ---- the files

fn entry(name: &str, dir: bool) -> DirEntry {
    DirEntry { name: name.to_string(), kind: if dir { crate::symbols_source::EntryKind::Dir } else { crate::symbols_source::EntryKind::File }, size: 10, mtime_ms: 0 }
}

fn nothing(_: &str, _: bool) -> FileState {
    FileState::None
}

#[test]
fn the_rows_of_a_folder_are_what_the_tree_shows_folders_first_and_no_hidden_file() {
    let entries = vec![entry("b.rs", false), entry("src", true), entry(".hidden", false), entry("A.md", false), entry("docs", true)];
    let rows = rows_of(7, "", 0, &entries, &nothing);
    let names: Vec<&str> = rows.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["docs", "src", "A.md", "b.rs"], "folders first, then files, and nothing hidden");
    assert_eq!((rows.request_id, rows.dir.as_str(), rows.offset, rows.more), (7, "", 0, false));
    assert_eq!(rows.entries[0].kind, EntryKind::Dir);
    assert_eq!(rows.entries[3].kind, EntryKind::File);
}

#[test]
fn a_folder_longer_than_a_frame_comes_in_pages() {
    let entries: Vec<DirEntry> = (0..MAX_ROWS + 20).map(|i| entry(&format!("f{i:04}.rs"), false)).collect();
    let first = rows_of(1, "src", 0, &entries, &nothing);
    assert_eq!(first.entries.len(), MAX_ROWS);
    assert!(first.more, "there are rows after these");
    let second = rows_of(2, "src", u32::try_from(MAX_ROWS).unwrap(), &entries, &nothing);
    assert_eq!(second.entries.len(), 20);
    assert!(!second.more);
    assert_eq!(second.offset as usize, MAX_ROWS);
    assert_eq!(second.entries[0].name, format!("f{MAX_ROWS:04}.rs"));
    let past = rows_of(3, "src", u32::MAX, &entries, &nothing);
    assert!(past.entries.is_empty() && !past.more, "an offset past the end is no rows");
    assert_eq!(past.offset as usize, MAX_ROWS + 20);
}

#[test]
fn a_name_the_tree_would_not_show_is_left_out_and_each_row_has_the_state_the_tree_has_for_it() {
    let at_the_cap = format!("{}.rs", "x".repeat(MAX_ENTRY_NAME_BYTES - 3));
    let over = format!("{}.rs", "y".repeat(MAX_ENTRY_NAME_BYTES + 100));
    let entries = vec![entry(&at_the_cap, false), entry(&over, false), entry("busy.rs", false), entry("sub", true)];
    let state = |name: &str, is_dir: bool| match (name, is_dir) {
        ("busy.rs", false) => FileState::Working,
        ("sub", true) => FileState::Clean,
        _ => FileState::None,
    };
    let rows = rows_of(1, "", 0, &entries, &state);
    let by_name = |name: &str| rows.entries.iter().find(|e| e.name == name).map(|e| e.state);
    assert_eq!(by_name("busy.rs"), Some(FileState::Working));
    assert_eq!(by_name("sub"), Some(FileState::Clean));
    assert!(rows.entries.iter().any(|e| e.name == at_the_cap), "a name at the cap is shown whole");
    assert!(!rows.entries.iter().any(|e| e.name.starts_with("yyy")), "a name past it is left out, as the tree leaves it out");
    assert!(rows.entries.iter().all(|e| e.name.len() <= MAX_ENTRY_NAME_BYTES));
}

#[test]
fn what_the_tree_marks_is_the_phones_colour() {
    use crate::chips::State;
    assert_eq!(file_state_of(Some(State::Working)), FileState::Working);
    assert_eq!(file_state_of(Some(State::Suggested)), FileState::Unresolved);
    assert_eq!(file_state_of(Some(State::Done)), FileState::Clean);
    assert_eq!(file_state_of(Some(State::Failed)), FileState::None);
    assert_eq!(file_state_of(Some(State::Draft)), FileState::None);
    assert_eq!(file_state_of(None), FileState::None);
}
