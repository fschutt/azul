//! What has been looked at: the pages seen, when a file is reviewed, the totals of a folder. No window, no clock of its
//! own: the time that passes is told to the model.

use super::*;
use crate::{
    chips::Counts,
    symbols_source::{DirEntry, EntryKind},
    tree::{Rules, Tree},
};

fn view(pages: &[(usize, f32)]) -> Vec<(usize, f32)> {
    pages.to_vec()
}

// ---- seen: a page is seen when half of it has been in view for a second and a half, all told ----

#[test]
fn a_page_in_view_for_a_second_and_a_half_is_seen_and_not_before() {
    let mut seen = Seen::new();
    for _ in 0..3 {
        assert!(!seen.watch(&view(&[(0, 1.0)]), 400), "1,200 ms: not yet");
    }
    assert!(!seen.is_seen(0));
    assert!(!seen.watch(&view(&[(0, 1.0)]), 299), "1,499 ms in all");
    assert!(seen.watch(&view(&[(0, 1.0)]), 1), "1,500 ms: seen, and the call says a page became seen");
    assert!(seen.is_seen(0));
    assert!(!seen.watch(&view(&[(0, 1.0)]), 400), "a page seen is seen once");
    assert_eq!(seen.count(), 1);
}

#[test]
fn a_page_less_than_half_in_view_is_not_being_read() {
    let mut seen = Seen::new();
    for _ in 0..40 {
        seen.watch(&view(&[(3, 0.49)]), 400);
    }
    assert!(!seen.is_seen(3), "the pages next to the one in the middle are only passing");
    assert_eq!(seen.pending_len(), 0, "and no time is kept for them");
    let mut got = false;
    for _ in 0..4 {
        got |= seen.watch(&view(&[(3, 0.5)]), 400);
    }
    assert!(got && seen.is_seen(3), "exactly half counts: 1,600 ms");
    assert!(!Seen::new().watch(&view(&[(3, f32::NAN)]), 400), "no number, no page in view");
}

#[test]
fn the_time_is_the_sum_over_the_visits_and_a_page_out_of_view_keeps_what_it_had() {
    let mut seen = Seen::new();
    seen.watch(&view(&[(7, 1.0)]), 400);
    seen.watch(&view(&[(7, 1.0)]), 400);
    seen.watch(&view(&[(8, 1.0)]), 400);
    seen.watch(&view(&[(8, 1.0)]), 400);
    assert!(!seen.is_seen(7) && !seen.is_seen(8));
    seen.watch(&view(&[(7, 1.0)]), 400);
    assert!(seen.watch(&view(&[(7, 1.0)]), 400), "800 ms before, 800 ms after: 1,600");
    assert!(seen.is_seen(7) && !seen.is_seen(8));
}

#[test]
fn several_pages_in_view_are_watched_together() {
    let mut seen = Seen::new();
    for _ in 0..4 {
        seen.watch(&view(&[(1, 1.0), (2, 0.6), (3, 0.2)]), 400);
    }
    assert!(seen.is_seen(1) && seen.is_seen(2) && !seen.is_seen(3));
    assert_eq!(seen.count(), 2);
}

#[test]
fn a_long_gap_between_two_looks_is_not_time_spent_reading() {
    // a window that was asleep, a machine that stalled: the step is cut at MAX_STEP_MS
    let mut seen = Seen::new();
    seen.watch(&view(&[(0, 1.0)]), 3_600_000);
    assert!(!seen.is_seen(0), "an hour between two ticks is at most {MAX_STEP_MS} ms of reading");
    for _ in 0..4 {
        seen.watch(&view(&[(0, 1.0)]), MAX_STEP_MS);
    }
    assert!(seen.is_seen(0));
}

#[test]
fn a_page_with_ink_is_seen_without_waiting() {
    let mut seen = Seen::new();
    assert!(seen.mark(4));
    assert!(!seen.mark(4), "marked once");
    assert!(seen.is_seen(4));
    seen.mark(MAX_PAGES + 5);
    assert_eq!(seen.count(), 1, "a page no file has is not kept");
}

#[test]
fn the_pages_half_way_to_seen_are_bounded() {
    let mut seen = Seen::new();
    for page in 0..1_000 {
        seen.watch(&view(&[(page, 1.0)]), 50);
    }
    assert!(seen.pending_len() <= MAX_PENDING, "{} pages partway", seen.pending_len());
    assert_eq!(seen.count(), 0);
}

#[test]
fn the_pages_seen_are_kept_as_ranges_and_come_back() {
    let mut seen = Seen::new();
    for p in [0, 1, 2, 7, 9, 10, 11, 4_000] {
        seen.mark(p);
    }
    assert_eq!(seen.ranges(), vec![(0, 2), (7, 7), (9, 11), (4_000, 4_000)]);
    let back = Seen::from_ranges(&seen.ranges());
    assert_eq!(back.ranges(), seen.ranges());
    assert_eq!(back.count(), 8);
    assert!(back.is_seen(10) && !back.is_seen(8));
}

#[test]
fn ranges_that_are_wrong_are_made_right_not_trusted() {
    let back = Seen::from_ranges(&[(5, 2), (10, 12), (11, 14), (0, MAX_PAGES + 99)]);
    // a range that runs backwards is the other way round, ranges that overlap are one, none runs past what a file can have
    assert!(back.is_seen(2) && back.is_seen(5), "the backwards range");
    assert!(back.is_seen(13) && back.is_seen(14), "the overlap");
    assert!(back.count() <= MAX_PAGES);
    assert!(!back.is_seen(MAX_PAGES));
}

#[test]
fn the_lines_of_the_pages_seen_count_the_last_page_by_what_it_holds() {
    let mut seen = Seen::new();
    seen.mark(0);
    seen.mark(2);
    // 100 lines: pages of 46, 46 and 8
    assert_eq!(seen.lines_seen(100), 46 + 8);
    assert_eq!(seen.lines_seen(40), 40, "a file shorter than a page");
    assert_eq!(seen.lines_seen(0), 0);
    seen.mark(5);
    assert_eq!(seen.lines_seen(100), 54, "a page past the end of the file has no lines");
}

#[test]
fn every_page_of_a_file_marked_at_once() {
    let mut seen = Seen::new();
    seen.mark_all(3);
    assert_eq!((seen.count(), seen.ranges()), (3, vec![(0, 2)]));
    seen.clear();
    assert_eq!(seen.count(), 0);
    assert!(seen.ranges().is_empty());
}

// ---- reviewed ----

fn counts(working: usize, suggested: usize, done: usize) -> Counts {
    Counts { working, suggested, done }
}

#[test]
fn a_file_is_reviewed_when_every_page_is_seen_and_every_conversation_is_resolved() {
    assert!(is_reviewed(3, 3, counts(0, 0, 0), false), "no conversation: the pages seen are enough");
    assert!(is_reviewed(3, 3, counts(0, 0, 4), false), "all done");
    assert!(!is_reviewed(3, 3, counts(1, 0, 0), false), "an agent is working");
    assert!(!is_reviewed(3, 3, counts(0, 1, 5), false), "a suggestion waits");
    assert!(!is_reviewed(2, 3, counts(0, 0, 0), false), "a page was not seen");
    assert!(!is_reviewed(0, 0, counts(0, 0, 0), false), "a file with no pages is nothing to have reviewed");
}

#[test]
fn what_the_reviewer_says_is_not_reviewed_is_not_whatever_the_pages_say() {
    assert!(!is_reviewed(3, 3, counts(0, 0, 0), true));
}

// ---- the totals of a folder ----

fn dir(name: &str) -> DirEntry {
    DirEntry { name: name.to_string(), kind: EntryKind::Dir, size: 0, mtime_ms: 0 }
}

fn file(name: &str) -> DirEntry {
    DirEntry { name: name.to_string(), kind: EntryKind::File, size: 10, mtime_ms: 1 }
}

/// root { src { a.rs, b.rs, deep { c.rs } }, d.rs }, listed.
fn tree() -> Tree {
    let mut t = Tree::new();
    t.list(t.root(), &[dir("src"), file("d.rs")], &Rules::default());
    let src = t.find("src").expect("src");
    t.list(src, &[file("a.rs"), file("b.rs"), dir("deep")], &Rules::default());
    let deep = t.find("src/deep").expect("deep");
    t.list(deep, &[file("c.rs")], &Rules::default());
    t
}

#[test]
fn the_totals_of_a_folder_are_those_of_the_files_under_it_down_to_the_last() {
    let t = tree();
    let info = |id: usize| match t.path(id).as_str() {
        "src/a.rs" => FileInfo { lines: Some(100), lines_seen: 100, reviewed: true },
        "src/b.rs" => FileInfo { lines: Some(50), lines_seen: 46, reviewed: false },
        "src/deep/c.rs" => FileInfo { lines: Some(10), lines_seen: 0, reviewed: false },
        "d.rs" => FileInfo { lines: Some(1_000), lines_seen: 1_000, reviewed: true },
        _ => FileInfo::default(),
    };
    let totals = rollup(&t, &info);
    let at = |path: &str| totals[t.find(path).expect(path)];
    assert_eq!(
        at("src"),
        Totals { files: 3, files_reviewed: 1, lines: 160, lines_reviewed: 100, lines_seen: 146, unknown_files: 0 }
    );
    assert_eq!(at("src/deep"), Totals { files: 1, files_reviewed: 0, lines: 10, lines_reviewed: 0, lines_seen: 0, unknown_files: 0 });
    assert_eq!(
        at(""),
        Totals { files: 4, files_reviewed: 2, lines: 1_160, lines_reviewed: 1_100, lines_seen: 1_146, unknown_files: 0 },
        "the root is the workspace"
    );
    assert_eq!(at("src/a.rs"), Totals { files: 1, files_reviewed: 1, lines: 100, lines_reviewed: 100, lines_seen: 100, unknown_files: 0 });
}

#[test]
fn a_file_whose_lines_are_not_counted_yet_is_counted_as_unknown_and_adds_no_lines() {
    let t = tree();
    let info = |id: usize| match t.path(id).as_str() {
        "src/a.rs" => FileInfo { lines: Some(100), lines_seen: 100, reviewed: true },
        "d.rs" => FileInfo { lines: None, lines_seen: 0, reviewed: false },
        _ => FileInfo { lines: None, ..FileInfo::default() },
    };
    let totals = rollup(&t, &info);
    let root = totals[t.root()];
    assert_eq!((root.files, root.lines, root.unknown_files), (4, 100, 3));
}

#[test]
fn a_tree_of_thousands_is_summed_in_one_pass() {
    let mut t = Tree::new();
    let top: Vec<DirEntry> = (0..30).map(|i| dir(&format!("d{i:02}"))).collect();
    t.list(t.root(), &top, &Rules::default());
    for i in 0..30 {
        let d = t.find(&format!("d{i:02}")).expect("dir");
        let inner: Vec<DirEntry> = (0..500).map(|j| file(&format!("f{j:03}.rs"))).collect();
        t.list(d, &inner, &Rules::default());
    }
    let info = |_: usize| FileInfo { lines: Some(10), lines_seen: 4, reviewed: false };
    let totals = rollup(&t, &info);
    assert_eq!(totals[t.root()].files, 15_000);
    assert_eq!(totals[t.root()].lines, 150_000);
    assert_eq!(totals[t.find("d07").expect("d07")].lines_seen, 2_000);
}

#[test]
fn the_summary_of_totals_is_said_in_numbers_the_window_can_show() {
    let t = Totals { files: 4, files_reviewed: 2, lines: 1_160, lines_reviewed: 1_100, lines_seen: 1_146, unknown_files: 0 };
    assert_eq!(t.percent_reviewed(), 94);
    assert_eq!(Totals::default().percent_reviewed(), 0, "no lines, no division by nothing");
    assert!(!t.is_partial());
    assert!(Totals { unknown_files: 1, ..t }.is_partial(), "some lines are not counted yet");
}

// ---- which pages are in view ----

#[test]
fn the_pages_in_view_are_those_the_viewport_reaches_each_with_the_part_of_it_that_is() {
    // pages 1,000 wide, 1,024 apart; a viewport of 1,200 px
    let at = |scroll: f32| pages_in_view(scroll, 1_200.0, 1_000.0, 1_024.0, 10);
    assert_eq!(at(0.0), vec![(0, 1.0), (1, (1_200.0 - 1_024.0) / 1_000.0)]);
    // scrolled by half a page: page 0 is half in view, page 1 two thirds (the viewport ends 676 px into it), page 2 not at all
    let half = at(500.0);
    assert_eq!(half.len(), 2, "{half:?}");
    assert_eq!(half[0].0, 0);
    assert!((half[0].1 - 0.5).abs() < 1e-4, "{half:?}");
    assert_eq!(half[1].0, 1);
    assert!((half[1].1 - 0.676).abs() < 1e-4, "{half:?}");
    // the gap between two pages shows no page
    let gap = pages_in_view(1_010.0, 10.0, 1_000.0, 1_024.0, 10);
    assert!(gap.is_empty(), "{gap:?}");
}

#[test]
fn the_pages_in_view_never_include_a_page_the_file_does_not_have_and_never_panic() {
    assert_eq!(pages_in_view(0.0, 5_000.0, 1_000.0, 1_024.0, 2).len(), 2);
    assert!(pages_in_view(0.0, 5_000.0, 1_000.0, 1_024.0, 0).is_empty());
    for (scroll, width, page, stride) in [
        (f32::NAN, 1_200.0, 1_000.0, 1_024.0),
        (-50.0, 1_200.0, 1_000.0, 1_024.0),
        (0.0, f32::INFINITY, 1_000.0, 1_024.0),
        (0.0, 1_200.0, 0.0, 1_024.0),
        (0.0, 1_200.0, 1_000.0, 0.0),
        (1.0e12, 1_200.0, 1_000.0, 1_024.0),
    ] {
        let seen = pages_in_view(scroll, width, page, stride, 10);
        assert!(seen.iter().all(|(p, f)| *p < 10 && (0.0..=1.0).contains(f)), "{scroll} {width}: {seen:?}");
    }
}

#[test]
fn a_page_only_half_shown_is_at_the_edge_of_being_read() {
    // the viewport ends in the middle of page 1: its half in view is enough
    let seen = pages_in_view(0.0, 1_024.0 + 500.0, 1_000.0, 1_024.0, 3);
    assert_eq!(seen.len(), 2);
    assert!(seen[1].1 >= SEEN_FRACTION - 1e-4, "{seen:?}");
}

// ---- the book: what is known of each file ----

#[test]
fn the_book_keeps_a_file_by_its_path_and_says_when_it_changed() {
    let mut book = Book::default();
    assert!(book.get("a.rs").is_none());
    assert!(!book.is_dirty());
    book.entry("a.rs").expect("room").seen.mark(2);
    book.set_dirty();
    assert!(book.is_dirty());
    assert_eq!(book.get("a.rs").map(|e| e.seen.count()), Some(1));
    assert_eq!(book.len(), 1);
    assert!(book.take_dirty() && !book.is_dirty(), "told once");
    assert!(!book.take_dirty());
}

#[test]
fn the_book_has_room_for_so_many_files_and_no_more() {
    let mut book = Book::with_cap(3);
    for name in ["a", "b", "c"] {
        assert!(book.entry(name).is_some());
    }
    assert!(book.entry("d").is_none(), "no room for a fourth");
    assert!(book.entry("a").is_some(), "a file that is there is still there");
    assert_eq!(book.len(), 3);
}

// ---- where the reviewer comes back to ----

fn here(file: &str, page: usize, line: u32) -> Back {
    Back { file: file.to_string(), page, line }
}

#[test]
fn the_first_jump_to_a_conversation_is_where_the_reviewer_comes_back_to_and_the_next_ones_keep_it() {
    let mut book = Book::default();
    assert!(book.back().is_none());
    assert!(book.jumped_from(here("src/lib.rs", 2, 93)), "the first jump sets it");
    assert!(book.is_dirty(), "and it is to be kept");
    assert_eq!(book.back(), Some(&here("src/lib.rs", 2, 93)));
    // jumping from one conversation to another is no new place to come back to
    assert!(!book.jumped_from(here("src/other.rs", 0, 1)));
    assert_eq!(book.back(), Some(&here("src/lib.rs", 2, 93)), "the first one stays");
}

#[test]
fn coming_back_gives_the_place_and_clears_it() {
    let mut book = Book::default();
    assert_eq!(book.come_back(), None, "nothing to come back to");
    book.jumped_from(here("a.rs", 1, 47));
    book.take_dirty();
    assert_eq!(book.come_back(), Some(here("a.rs", 1, 47)));
    assert!(book.back().is_none());
    assert!(book.is_dirty(), "the clearing is kept too");
    assert_eq!(book.come_back(), None);
    // and the next jump sets a new one
    assert!(book.jumped_from(here("b.rs", 0, 1)));
}

#[test]
fn a_place_is_told_in_words_with_the_page_and_the_line() {
    assert_eq!(here("src/lib.rs", 2, 93).words(), "src/lib.rs p3 l93", "pages are counted from one for the reviewer");
}

#[test]
fn a_place_with_a_file_name_no_one_could_have_is_not_kept() {
    let mut book = Book::default();
    assert!(!book.jumped_from(here("", 0, 1)), "no file, no place");
    assert!(!book.jumped_from(here(&"x".repeat(MAX_BACK_PATH + 1), 0, 1)), "a path longer than a path can be");
    assert!(book.back().is_none());
}
