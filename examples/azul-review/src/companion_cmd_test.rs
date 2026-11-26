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
    let state = pen_state_of(&s, Some(&strip()));
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
    let state = pen_state_of(&s, Some(&strip()));
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
    let line = pen_state_of(&s, Some(&strip())).page().map(|p| p.line);
    assert_eq!(line, Some(0), "line 91 + 40 is past line 120");
}

#[test]
fn the_state_has_the_zoom_the_pages_are_drawn_at_and_the_place_to_go_back_to() {
    let mut s = app_with_file();
    s.zoom = 2.0;
    let zoomed = ui::StripNow { scroll: (0.0, 0.0), size: (1100.0, 700.0) };
    let state = pen_state_of(&s, Some(&zoomed));
    assert!(near(state.view.w, 0.55), "{:?}", state.view);
    let _ = s.progress.jumped_from(crate::progress::Back { file: "f.rs".to_string(), page: 1, line: 50 });
    let back = pen_state_of(&s, Some(&zoomed)).return_point.expect("the way back");
    assert_eq!((back.file.as_str(), back.page_index, back.line), ("f.rs", 1, 50));
    assert!(back.label.contains("f.rs"), "{back:?}");
}

#[test]
fn with_no_file_or_no_strip_the_phone_is_told_the_folder_and_an_empty_page() {
    let none = AppState::create(PathBuf::from("/work/proj"), Vec::new(), RefAny::new(0u8), PathBuf::new());
    let state = pen_state_of(&none, Some(&strip()));
    assert_eq!(state.folder, "proj");
    assert_eq!(state.page().map(|p| p.file.as_str()), Some(""));
    let unlaid = pen_state_of(&app_with_file(), None);
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
