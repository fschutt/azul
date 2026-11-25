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
