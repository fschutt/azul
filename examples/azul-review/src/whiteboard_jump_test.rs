//! The jump from a cutout back to its lines: where to go, and whether what is there is what was
//! circled. No window, no azul.

use std::{collections::BTreeMap, sync::Arc};

use super::*;
use crate::whiteboard::{kept_lines, lines_hash, Board, CutoutDraft, CutoutImage, Source, LIMITS};
use crate::whiteboard_geom::Pt;

/// A workspace of files held as lines.
#[derive(Default)]
struct Files(BTreeMap<String, Vec<String>>);

impl Files {
    fn with(mut self, name: &str, lines: &[&str]) -> Self {
        self.0.insert(name.to_string(), lines.iter().map(|l| (*l).to_string()).collect());
        self
    }
}

impl FileView for Files {
    fn file_hash(&self, file: &str) -> Option<u64> {
        self.0.get(file).map(|l| lines_hash(l))
    }

    fn line_count(&self, file: &str) -> Option<u32> {
        self.0.get(file).map(|l| u32::try_from(l.len()).unwrap_or(u32::MAX))
    }

    fn lines(&self, file: &str, first: u32, last: u32) -> Option<Vec<String>> {
        let all = self.0.get(file)?;
        let (a, b) = (first.saturating_sub(1) as usize, last as usize);
        Some(all.get(a..b.min(all.len()))?.to_vec())
    }

    fn find_block(&self, file: &str, block: &[String], near: u32) -> Option<u32> {
        find_block_in(self.0.get(file)?, block, near)
    }
}

fn text(n: usize) -> Vec<String> {
    (1..=n).map(|i| format!("fn line_{i}() {{}}")).collect()
}

fn as_refs(v: &[String]) -> Vec<&str> {
    v.iter().map(String::as_str).collect()
}

/// A cutout of lines `first` to `last` of `file`, made when the file was `then`.
fn cutout_of(file: &str, first: u32, last: u32, then: &[String]) -> Cutout {
    let covered: Vec<String> = kept_lines(&then[(first as usize - 1)..last as usize], &LIMITS);
    let mut board = Board::new();
    let image = Arc::new(CutoutImage { width: 2, height: 2, scale: 1.0, rgba: vec![255; 16], png: vec![] });
    let source = Source {
        file: file.to_string(),
        page: 0,
        first_line: first,
        last_line: last,
        shape: vec![Pt::new(0.0, 0.0), Pt::new(10.0, 0.0), Pt::new(10.0, 10.0)],
        file_hash: lines_hash(then),
        range_hash: lines_hash(&covered),
        lines: covered,
    };
    let id = board.add_cutout(CutoutDraft::ready(Pt::default(), image, source)).expect("added");
    board.cutout(id).cloned().expect("there")
}

#[test]
fn a_file_that_is_as_it_was_is_a_fresh_jump_to_the_circled_lines() {
    let then = text(40);
    let c = cutout_of("src/a.rs", 10, 12, &then);
    let files = Files::default().with("src/a.rs", &as_refs(&then));
    let t = jump_target(&c, &files);
    assert_eq!((t.file.as_str(), t.first_line, t.last_line), ("src/a.rs", 10, 12));
    assert_eq!(t.freshness, Freshness::Fresh);
    assert_eq!(t.id, c.id);
    assert_eq!(t.page(46), 0);
}

#[test]
fn a_file_that_is_gone_is_still_a_jump_to_where_it_was_and_says_so() {
    let then = text(40);
    let c = cutout_of("src/a.rs", 10, 12, &then);
    let t = jump_target(&c, &Files::default());
    assert_eq!(t.freshness, Freshness::FileMissing);
    assert_eq!((t.first_line, t.last_line), (10, 12));
}

#[test]
fn a_file_changed_elsewhere_is_still_a_jump_to_the_same_lines_and_says_the_file_changed() {
    let then = text(40);
    let c = cutout_of("src/a.rs", 10, 12, &then);
    let mut now = then.clone();
    now[30] = "fn added_far_away() {}".to_string();
    let files = Files::default().with("src/a.rs", &as_refs(&now));
    let t = jump_target(&c, &files);
    assert_eq!(t.freshness, Freshness::FileChanged);
    assert_eq!((t.first_line, t.last_line), (10, 12));
}

#[test]
fn lines_that_moved_are_found_where_they_are_now() {
    let then = text(40);
    let c = cutout_of("src/a.rs", 10, 12, &then);
    let mut now = vec!["// a new header".to_string(); 7];
    now.extend(then.clone());
    let files = Files::default().with("src/a.rs", &as_refs(&now));
    let t = jump_target(&c, &files);
    assert_eq!(t.freshness, Freshness::LinesMoved);
    assert_eq!((t.first_line, t.last_line), (17, 19), "seven lines further down");
    assert_eq!(t.page(46), 0);
}

#[test]
fn of_two_places_the_lines_are_found_at_the_nearer_wins() {
    let then = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    let c = cutout_of("x.rs", 40, 41, &{
        let mut v = vec!["pad".to_string(); 39];
        v.extend(then.clone());
        v
    });
    // Now the two lines "a", "b" are at 5 and at 60: 40 is nearer to 60 than to 5? 35 against 20: the nearer is 60.
    let mut now = vec!["pad".to_string(); 80];
    now[4] = "a".to_string();
    now[5] = "b".to_string();
    now[59] = "a".to_string();
    now[60] = "b".to_string();
    // The captured lines are "a" and "b" of the original (lines 40 and 41).
    let files = Files::default().with("x.rs", &as_refs(&now));
    let t = jump_target(&c, &files);
    assert_eq!(t.freshness, Freshness::LinesMoved);
    assert_eq!(t.first_line, 60);
}

#[test]
fn lines_that_changed_and_are_nowhere_are_a_jump_to_the_old_place_that_says_so() {
    let then = text(40);
    let c = cutout_of("src/a.rs", 10, 12, &then);
    let mut now = then.clone();
    now[10] = "fn rewritten() {}".to_string();
    let files = Files::default().with("src/a.rs", &as_refs(&now));
    let t = jump_target(&c, &files);
    assert_eq!(t.freshness, Freshness::LinesChanged);
    assert_eq!((t.first_line, t.last_line), (10, 12));
}

#[test]
fn a_file_that_became_shorter_than_the_lines_is_a_changed_jump_clamped_to_its_end() {
    let then = text(40);
    let c = cutout_of("src/a.rs", 30, 35, &then);
    let now = text(12);
    let files = Files::default().with("src/a.rs", &as_refs(&now));
    let t = jump_target(&c, &files);
    assert_eq!(t.freshness, Freshness::LinesChanged);
    assert!(t.first_line <= 12 && t.last_line <= 12 && t.first_line <= t.last_line, "{t:?}");
}

#[test]
fn the_page_of_a_jump_is_the_page_the_first_line_is_on() {
    let then = text(200);
    let c = cutout_of("src/a.rs", 100, 110, &then);
    let files = Files::default().with("src/a.rs", &as_refs(&then));
    let t = jump_target(&c, &files);
    assert_eq!(t.page(46), 2, "line 100 is on the third page of 46 lines");
    assert_eq!(t.page(0), 0, "no page size, no page");
}

#[test]
fn finding_a_block_looks_for_the_whole_of_it_and_the_nearest_of_the_places() {
    let lines: Vec<String> = ["x", "a", "b", "y", "z", "a", "b", "c"].iter().map(|s| (*s).to_string()).collect();
    let ab = vec!["a".to_string(), "b".to_string()];
    assert_eq!(find_block_in(&lines, &ab, 1), Some(2));
    assert_eq!(find_block_in(&lines, &ab, 8), Some(6));
    assert_eq!(find_block_in(&lines, &ab, 4), Some(2), "equally near: the earlier");
    assert_eq!(find_block_in(&lines, &["a".to_string(), "c".to_string()], 1), None);
    assert_eq!(find_block_in(&lines, &[], 1), None, "no lines, nothing to find");
    assert_eq!(find_block_in(&[], &ab, 1), None);
    let too_long = vec!["x".to_string(); 20];
    assert_eq!(find_block_in(&lines, &too_long, 1), None);
}

#[test]
fn a_jump_is_heard_by_the_observer() {
    use std::{cell::RefCell, rc::Rc};
    struct Log(Rc<RefCell<Vec<crate::whiteboard::BoardEvent>>>);
    impl crate::whiteboard::BoardObserver for Log {
        fn on_event(&mut self, e: &crate::whiteboard::BoardEvent) {
            self.0.borrow_mut().push(e.clone());
        }
    }
    let then = text(40);
    let heard = Rc::new(RefCell::new(Vec::new()));
    let mut board = Board::new();
    board.set_observer(Box::new(Log(heard.clone())));
    let image = Arc::new(CutoutImage { width: 2, height: 2, scale: 1.0, rgba: vec![255; 16], png: vec![] });
    let source = Source {
        file: "src/a.rs".to_string(),
        page: 0,
        first_line: 10,
        last_line: 12,
        shape: vec![],
        file_hash: lines_hash(&then),
        range_hash: lines_hash(&then[9..12]),
        lines: then[9..12].to_vec(),
    };
    let id = board.add_cutout(CutoutDraft::ready(Pt::default(), image, source)).expect("added");
    let files = Files::default().with("src/a.rs", &as_refs(&then));
    let target = jump(&mut board, id, &files).expect("a jump");
    assert_eq!(target.first_line, 10);
    assert_eq!(
        heard.borrow().last(),
        Some(&crate::whiteboard::BoardEvent::Jumped { id, file: "src/a.rs".to_string(), first_line: 10, last_line: 12 })
    );
    assert!(jump(&mut board, 9_999, &files).is_err(), "no such cutout");
}

#[test]
fn lines_longer_than_the_board_keeps_are_compared_as_far_as_it_kept_them() {
    let mut then = text(40);
    then[10] = "x".repeat(LIMITS.max_source_line_chars + 200);
    let c = cutout_of("src/a.rs", 10, 12, &then);
    assert!(c.source.lines.iter().all(|l| l.chars().count() <= LIMITS.max_source_line_chars));
    let mut now = then.clone();
    now[30] = "fn elsewhere() {}".to_string();
    let files = Files::default().with("src/a.rs", &as_refs(&now));
    assert_eq!(jump_target(&c, &files).freshness, Freshness::FileChanged, "the long line is as it was");
    // A change inside what was kept is a change; one past what was kept is not seen.
    let mut changed = now.clone();
    changed[10] = format!("y{}", &then[10][1..]);
    let files = Files::default().with("src/a.rs", &as_refs(&changed));
    assert_eq!(jump_target(&c, &files).freshness, Freshness::LinesChanged);
}

#[test]
fn what_the_person_is_told_of_a_jump_that_is_not_fresh() {
    let said = |f: Freshness| f.key();
    assert_eq!(said(Freshness::Fresh), None, "a fresh jump says nothing");
    for f in [Freshness::FileChanged, Freshness::LinesMoved, Freshness::LinesChanged, Freshness::FileMissing] {
        assert!(said(f).is_some_and(|k| k.contains("whiteboard-jump-")), "{f:?}");
    }
    assert!(Freshness::LinesMoved.english(12, 14).contains("12") && Freshness::LinesMoved.english(12, 14).contains("14"));
}
