//! The findings the ink derives: which lines of the file a stroke is over. No window.

use super::*;
use crate::model::InkPoint;

fn file() -> code::SourceFile {
    code::SourceFile {
        path: PathBuf::from("f.rs"),
        display: "f.rs".to_string(),
        lines: (0..120).map(|i| format!("fn f{i}() {{}}")).collect(),
    }
}

fn stroke_at(page: usize, y: f32) -> Stroke {
    let point = |x: f32| InkPoint { x, y, pressure: 0.5, tilt_x: 0.0, tilt_y: 0.0 };
    Stroke {
        page,
        semantic: Semantic::Issue,
        points: vec![point(40.0), point(90.0)],
        id: 1,
        epoch: 0,
    }
}

#[test]
fn a_stroke_over_the_third_line_of_a_page_is_a_finding_on_that_line() {
    // The code starts under the page's padding: the third line (number 3) is the one whose box
    // begins 2 lines below it.
    let y = ui::PAGE_PAD + 2.0 * ui::LINE_H + 5.0;
    let findings = derive_findings(&[stroke_at(0, y)], &file(), &None);
    assert_eq!(findings.len(), 1);
    assert_eq!((findings[0].first_line, findings[0].last_line), (3, 3));
}

#[test]
fn a_stroke_on_a_later_page_counts_from_that_pages_first_line() {
    // Page 1 of a file with 46 lines to a page starts at line 47.
    let y = ui::PAGE_PAD + 1.0;
    let findings = derive_findings(&[stroke_at(1, y)], &file(), &None);
    assert_eq!(findings[0].first_line, code::LINES_PER_PAGE + 1);
}

#[test]
fn ink_in_the_pages_padding_above_the_code_is_on_the_first_line() {
    let findings = derive_findings(&[stroke_at(0, 4.0)], &file(), &None);
    assert_eq!(findings[0].first_line, 1);
}

// ---- undo and redo ----

fn app_with_file() -> AppState {
    AppState::create(PathBuf::new(), vec![file()], RefAny::new(0u8), PathBuf::new())
}

fn kept(id: u64, page: usize) -> Stroke {
    Stroke { id, ..stroke_at(page, ui::PAGE_PAD + 1.0) }
}

#[test]
fn undo_takes_the_last_stroke_back_and_redo_puts_it_where_it_was() {
    let mut s = app_with_file();
    s.keep(kept(1, 0));
    s.keep(kept(2, 0));
    assert_eq!(s.strokes.len(), 2);

    assert!(s.undo(), "something to undo");
    assert_eq!(s.strokes.iter().map(|st| st.id).collect::<Vec<_>>(), vec![1]);
    assert_eq!(s.findings.len(), 1, "the findings follow the ink");

    assert!(s.redo(), "something to redo");
    assert_eq!(s.strokes.iter().map(|st| st.id).collect::<Vec<_>>(), vec![1, 2]);
    assert!(!s.redo(), "nothing more to redo");
}

#[test]
fn undo_with_nothing_drawn_does_nothing() {
    let mut s = app_with_file();
    assert!(!s.undo());
    assert!(!s.redo());
}

#[test]
fn a_new_stroke_ends_the_branch_redo_would_have_brought_back() {
    let mut s = app_with_file();
    s.keep(kept(1, 0));
    assert!(s.undo());
    s.keep(kept(2, 0));
    assert!(!s.redo(), "the stroke undone is gone for good once a new one is drawn");
    assert_eq!(s.strokes.iter().map(|st| st.id).collect::<Vec<_>>(), vec![2]);
}

#[test]
fn undoing_an_erase_puts_the_stroke_back_in_its_place() {
    let mut s = app_with_file();
    s.keep(kept(1, 0));
    s.keep(kept(2, 0));
    s.keep(kept(3, 0));
    assert!(s.erase(1).is_some(), "the middle stroke");
    assert_eq!(s.strokes.iter().map(|st| st.id).collect::<Vec<_>>(), vec![1, 3]);
    assert!(s.undo());
    assert_eq!(s.strokes.iter().map(|st| st.id).collect::<Vec<_>>(), vec![1, 2, 3]);
    assert!(s.redo());
    assert_eq!(s.strokes.iter().map(|st| st.id).collect::<Vec<_>>(), vec![1, 3]);
}

#[test]
fn undo_redraws_the_page_the_stroke_is_on() {
    let mut s = app_with_file();
    s.keep(kept(1, 1));
    let before = s.ink.key(1, (10, 10));
    assert!(s.undo());
    assert_ne!(s.ink.key(1, (10, 10)), before, "page 1's frame is drawn anew");
}

#[test]
fn opening_another_file_forgets_what_could_be_undone() {
    let mut s = AppState::create(
        PathBuf::new(),
        vec![file(), file()],
        RefAny::new(0u8),
        PathBuf::new(),
    );
    s.keep(kept(1, 0));
    s.open_file(1);
    assert!(!s.undo(), "the stroke belongs to the file left");
}

// ---- the eraser ----

#[test]
fn the_eraser_takes_the_stroke_it_touches_not_one_whose_box_it_is_inside() {
    // A long diagonal: its bounding box covers the page's corner, its line does not.
    let diagonal = Stroke {
        points: vec![
            InkPoint { x: 10.0, y: 10.0, pressure: 0.5, tilt_x: 0.0, tilt_y: 0.0 },
            InkPoint { x: 410.0, y: 410.0, pressure: 0.5, tilt_x: 0.0, tilt_y: 0.0 },
        ],
        ..kept(1, 0)
    };
    assert!(diagonal.is_near(210.0, 212.0, 6.0), "on the line");
    assert!(!diagonal.is_near(10.0, 400.0, 6.0), "inside the box, far from the line");
    assert!(diagonal.is_near(10.0, 10.0, 6.0), "at its start");
    assert!(!diagonal.is_near(500.0, 500.0, 6.0), "past its end");

    let mut s = app_with_file();
    s.keep(diagonal);
    assert_eq!(s.eraser_target(0, 10.0, 400.0), None, "the box alone is no hit");
    assert_eq!(s.eraser_target(0, 100.0, 100.0), Some(0));
    assert_eq!(s.eraser_target(1, 100.0, 100.0), None, "another page");
}

#[test]
fn a_one_point_stroke_is_hit_around_its_dab() {
    let dab = Stroke {
        points: vec![InkPoint { x: 50.0, y: 50.0, pressure: 0.5, tilt_x: 0.0, tilt_y: 0.0 }],
        ..kept(1, 0)
    };
    assert!(dab.is_near(53.0, 50.0, 6.0));
    assert!(!dab.is_near(70.0, 50.0, 6.0));
}

#[test]
fn the_eraser_takes_the_topmost_of_two_strokes_it_touches() {
    let mut s = app_with_file();
    s.keep(kept(1, 0));
    s.keep(kept(2, 0));
    // Both lie over each other: the one drawn last is on top.
    assert_eq!(s.eraser_target(0, 60.0, ui::PAGE_PAD + 1.0), Some(1));
}
