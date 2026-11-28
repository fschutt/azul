//! What the phone is told of the review, without a window: the view of the window, where the pen is in the column's units, what a button
//! would act on, and the place to come back to.

use super::*;

fn geometry() -> Geometry {
    Geometry { page_w: 1000.0, page_h: 760.0, gap: 24.0, pages: 4, window: (1100.0, 700.0) }
}

fn input<'a>() -> Input<'a> {
    Input {
        board: None,
        folder: "azul",
        file: "src/lib.rs",
        line: 57,
        line_count: 180,
        geometry: geometry(),
        looking: Looking::HOME,
        pen: Some((PenAt { page: 0, x: 500.0, y: 300.0, down: false }, 10)),
        region: false,
        back: None,
    }
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

#[test]
fn the_state_names_the_folder_the_file_the_pages_and_the_window() {
    let state = pen_state(&input());
    assert_eq!(state.folder, "azul");
    assert_eq!(state.screen, ScreenId::MAIN);
    let page = state.page().expect("the pages");
    assert_eq!((page.file.as_str(), page.page_index, page.page_count, page.line, page.line_count), ("src/lib.rs", 0, 4, 57, 180));
    assert!(near(page.aspect, 1000.0 / 760.0) && near(page.gap, 0.024), "{page:?}");
    assert!(near(state.view.w, 1.1) && near(state.view.h, 0.7), "{:?}", state.view);
    assert!(near(state.zoom, 1.0 / 1.1), "the phone's zoom of 1 is the width of a sheet in the window: {}", state.zoom);
    assert_eq!(state.seq, 0, "the host numbers them");
    assert_eq!(state.thumb_id, 0);
    assert_eq!(state.return_point, None);
}

#[test]
fn the_pen_is_in_the_units_of_the_column_whichever_page_it_is_on() {
    let mut i = input();
    i.pen = Some((PenAt { page: 2, x: 250.0, y: 380.0, down: true }, 0));
    let mark = pen_state(&i).pen.expect("the pen");
    assert!(near(mark.x, 0.25), "{mark:?}");
    assert!(near(mark.y, 2.0 * (760.0 + 24.0) / 1000.0 + 0.38), "{mark:?}");
    assert!(mark.down);
}

#[test]
fn a_pen_that_was_last_seen_a_while_ago_is_away_and_the_buttons_act_on_the_page() {
    let mut i = input();
    i.pen = Some((PenAt { page: 0, x: 10.0, y: 10.0, down: false }, PEN_AWAY_MS));
    assert!(pen_state(&i).pen.is_some(), "at the limit it is still here");
    i.pen = Some((PenAt { page: 0, x: 10.0, y: 10.0, down: false }, PEN_AWAY_MS + 1));
    let away = pen_state(&i);
    assert_eq!(away.pen, None);
    assert_eq!(away.context, Context::Page);
    assert_eq!(away.page().map(|p| p.line), Some(0), "no line under a pen that is not there");
    i.pen = None;
    assert_eq!(pen_state(&i).pen, None);
}

#[test]
fn a_button_acts_on_the_marked_region_else_the_line_under_the_pen_else_the_page() {
    assert_eq!(context_of(true, 12), Context::Region);
    assert_eq!(context_of(true, 0), Context::Region);
    assert_eq!(context_of(false, 12), Context::Line);
    assert_eq!(context_of(false, 0), Context::Page);
    let mut i = input();
    assert_eq!(pen_state(&i).context, Context::Line);
    i.region = true;
    assert_eq!(pen_state(&i).context, Context::Region);
}

#[test]
fn the_window_zoomed_in_is_a_smaller_view_and_a_greater_zoom() {
    let mut i = input();
    i.looking = Looking { zoom: 2.0, page: 1, at: (100.0, 50.0) };
    let state = pen_state(&i);
    assert!(near(state.view.w, 0.55) && near(state.zoom, 1.0 / 0.55), "{:?} {}", state.view, state.zoom);
    assert!(near(state.view.x, 0.1), "{:?}", state.view);
    assert!(near(state.view.y, (760.0 + 24.0) / 1000.0 + 0.05), "{:?}", state.view);
    assert_eq!(state.page().map(|p| p.page_index), Some(1));
}

#[test]
fn the_place_to_come_back_to_is_told_with_its_few_words() {
    let mut i = input();
    i.back = Some(("src/main.rs".to_string(), 2, 93, "src/main.rs p3 l93".to_string()));
    let back = pen_state(&i).return_point.expect("a return point");
    assert_eq!((back.file.as_str(), back.page_index, back.line, back.label.as_str()), ("src/main.rs", 2, 93, "src/main.rs p3 l93"));
}

#[test]
fn what_is_said_of_the_folder_and_the_files_is_cleaned_and_cut() {
    let mut i = input();
    let long = "x".repeat(5_000);
    i.folder = "fol\nder\u{7}";
    i.file = &long;
    let state = pen_state(&i);
    assert_eq!(state.folder, "folder");
    assert!(state.page().is_some_and(|p| p.file.len() <= MAX_PATH_BYTES), "cut to what a path may be");
    i.back = Some((long.clone(), 0, 1, long.clone()));
    let back = pen_state(&i).return_point.expect("a return point");
    assert!(back.file.len() <= MAX_PATH_BYTES && back.label.len() <= MAX_NAME_BYTES);
}

#[test]
fn a_geometry_that_is_no_geometry_still_makes_a_state_of_numbers() {
    let mut i = input();
    i.geometry = Geometry { page_w: f32::NAN, page_h: 0.0, gap: f32::NAN, pages: 0, window: (0.0, 0.0) };
    i.looking = Looking { zoom: f32::NAN, page: usize::MAX, at: (f32::NAN, f32::INFINITY) };
    let state = pen_state(&i);
    assert!(state.view.x.is_finite() && state.view.y.is_finite() && state.view.w > 0.0 && state.view.h > 0.0, "{:?}", state.view);
    assert!(state.zoom.is_finite() && state.zoom > 0.0);
}

// ---- over the whiteboard

fn over_board<'a>() -> Input<'a> {
    let mut i = input();
    i.board = Some(BoardInput {
        geometry: board_view::Geometry { items: Some(crate::whiteboard_geom::Rect { x: 100.0, y: 50.0, w: 2000.0, h: 1000.0 }), window: (800.0, 600.0), zoom: (0.2, 4.0) },
        view: View { pan: Pt::new(600.0, 300.0), zoom: 0.5 },
        board_id: 3,
        name: "Architecture".to_string(),
        items: 7,
        pointer: Some(Pt::new(1100.0, 550.0)),
    });
    i
}

#[test]
fn over_the_whiteboard_the_state_is_the_boards_name_and_shape_and_the_window_of_it() {
    let state = pen_state(&over_board());
    let board = state.board().expect("the whiteboard");
    assert_eq!((board.board_id, board.name.as_str(), board.item_count), (3, "Architecture", 7));
    assert!(near(board.aspect, 2.0), "{board:?}");
    assert!(state.page().is_none(), "no pages here");
    assert!(near(state.view.x, 0.25) && near(state.view.y, 0.125) && near(state.view.w, 0.8) && near(state.view.h, 0.6), "{:?}", state.view);
    assert!(near(state.zoom, 1.25), "{}", state.zoom);
    assert_eq!(state.folder, "azul");
}

#[test]
fn over_the_whiteboard_the_pen_is_the_pointer_in_the_boxs_units_and_a_button_acts_on_the_region_or_the_page() {
    let mut i = over_board();
    let state = pen_state(&i);
    let mark = state.pen.expect("the pointer is over the board");
    assert!(near(mark.x, 0.5) && near(mark.y, 0.25), "{mark:?}");
    assert_eq!(state.context, Context::Page, "no line on a board");
    i.region = true;
    assert_eq!(pen_state(&i).context, Context::Region);
    if let Some(board) = i.board.as_mut() {
        board.pointer = None;
    }
    assert_eq!(pen_state(&i).pen, None);
}

#[test]
fn the_place_to_come_back_to_is_told_over_the_whiteboard_too() {
    let mut i = over_board();
    i.back = Some(("src/lib.rs".to_string(), 1, 50, "src/lib.rs p2 l50".to_string()));
    assert_eq!(pen_state(&i).return_point.map(|r| r.line), Some(50));
}
