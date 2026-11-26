//! The demo window's own pieces: the file it shows, the bar's place, the file view the jump asks. Needs azul, no window.

use super::*;

#[test]
fn the_demo_shows_a_page_of_its_own_source() {
    let (name, lines) = demo_text();
    assert_eq!(name, "whiteboard_demo.rs");
    assert_eq!(lines.len(), 46);
    assert!(lines[0].starts_with("//! A window to look at the whiteboard in"));
}

#[test]
fn the_bar_is_the_right_edge_of_the_window() {
    let bar = bar_of(Size::new(1_400.0, 800.0));
    assert!(bar.open);
    assert!((bar.rect.right() - 1_400.0).abs() < 1e-3 && (bar.rect.w - BAR_W).abs() < 1e-3);
    assert_eq!(bar.window_w, 1_400.0);
}

#[test]
fn the_one_file_answers_the_jump_about_itself_and_nothing_else() {
    let lines: Vec<String> = ["a", "b", "c", "b"].iter().map(|s| (*s).to_string()).collect();
    let files = OneFile { name: "x.rs", lines: &lines };
    assert_eq!(files.line_count("x.rs"), Some(4));
    assert_eq!(files.line_count("y.rs"), None);
    assert_eq!(files.lines("x.rs", 2, 3), Some(vec!["b".to_string(), "c".to_string()]));
    assert_eq!(files.lines("x.rs", 3, 9), None, "past the end");
    assert_eq!(files.find_block("x.rs", &["b".to_string()], 4), Some(4));
    assert_eq!(files.file_hash("x.rs"), Some(lines_hash(&lines)));
    assert_eq!(files.file_hash("y.rs"), None);
}
