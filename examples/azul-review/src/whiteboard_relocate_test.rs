//! Finding the code a cutout points at again after the code moved on: unchanged, moved, changed, vanished. No window, no azul.

use std::collections::BTreeMap;

use super::*;
use crate::{whiteboard::LIMITS, whiteboard_jump::find_block_in};

const NOW: &str = "2222222222222222222222222222222222222222";
const THEN: &str = "1111111111111111111111111111111111111111";

/// A workspace of files held as lines.
#[derive(Default)]
struct Files(BTreeMap<String, Vec<String>>);

impl Files {
    fn with(mut self, name: &str, lines: &[String]) -> Self {
        self.0.insert(name.to_string(), lines.to_vec());
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

/// A file of `n` distinct lines.
fn file(n: usize) -> Vec<String> {
    (1..=n).map(|i| format!("    let value_{i} = compute_{i}();")).collect()
}

/// A reference to lines `first` to `last` of `name` as `then` had them.
fn reference(name: &str, first: u32, last: u32, then: &[String]) -> Source {
    let covered = then[(first as usize - 1)..last as usize].to_vec();
    let kept = kept_lines(&covered, &LIMITS);
    Source {
        file: name.to_string(),
        page: 0,
        first_line: first,
        last_line: last,
        shape: Vec::new(),
        file_hash: lines_hash(then),
        range_hash: lines_hash(&kept),
        sha256: whiteboard_hash::lines_tagged(&covered),
        lines: kept,
        rev: THEN.to_string(),
    }
}

fn go(source: &Source, now: &Files) -> Relocation {
    relocate(source, now, Some(NOW), &LIMITS)
}

#[test]
fn a_file_that_is_as_it_was_leaves_the_reference_where_it_was_and_names_the_new_revision() {
    let then = file(40);
    let s = reference("a.rs", 10, 14, &then);
    let r = go(&s, &Files::default().with("a.rs", &then));
    assert_eq!((r.status, r.from, r.to), (RefStatus::Unchanged, (10, 14), (10, 14)));
    assert_eq!(r.source.rev, NOW, "the revision the code is at");
    assert_eq!((r.source.lines.clone(), r.source.sha256.clone(), r.source.range_hash), (s.lines.clone(), s.sha256.clone(), s.range_hash), "what it said is what it says");
    assert_eq!((r.source.first_line, r.source.last_line), (10, 14));
}

#[test]
fn lines_that_are_intact_in_a_file_that_changed_elsewhere_are_unchanged() {
    let then = file(40);
    let s = reference("a.rs", 10, 14, &then);
    let mut now = then.clone();
    now[30] = "    something else entirely();".to_string();
    now.push("    a new last line();".to_string());
    let r = go(&s, &Files::default().with("a.rs", &now));
    assert_eq!((r.status, r.to), (RefStatus::Unchanged, (10, 14)));
    assert_eq!(r.source.file_hash, lines_hash(&now), "the file as it is now");
}

#[test]
fn lines_that_moved_are_found_by_what_they_say_and_the_reference_follows_them() {
    let then = file(40);
    let s = reference("a.rs", 10, 14, &then);
    let mut now: Vec<String> = (0..7).map(|i| format!("    // inserted above, line {i}")).collect();
    now.extend(then.iter().cloned());
    let r = go(&s, &Files::default().with("a.rs", &now));
    assert_eq!((r.status, r.from, r.to), (RefStatus::Moved, (10, 14), (17, 21)));
    assert_eq!(r.source.lines, now[16..21].to_vec());
    assert_eq!(r.source.sha256, whiteboard_hash::lines_tagged(&now[16..21]), "the hash of what is at the new place");
    assert_eq!(r.source.rev, NOW);
    // The nearest of two places wins.
    let mut twice = now.clone();
    twice.extend(then[9..14].iter().cloned());
    assert_eq!(go(&s, &Files::default().with("a.rs", &twice)).to, (17, 21));
}

#[test]
fn lines_that_changed_inside_are_found_by_their_first_and_last_and_say_what_is_there_now() {
    let then = file(40);
    let s = reference("a.rs", 10, 20, &then);
    let mut now = then.clone();
    now.insert(0, "    // a line above".to_string());
    now[15] = "    let rewritten = in_the_middle();".to_string();
    now.insert(16, "    let one_more = line();".to_string());
    let r = go(&s, &Files::default().with("a.rs", &now));
    assert_eq!(r.status, RefStatus::Changed);
    assert_eq!((r.from, r.to), ((10, 20), (11, 22)), "from the first line that is found to the last");
    assert_eq!(r.source.lines, now[10..22].to_vec(), "what is there now, not what was there");
    assert_ne!(r.source.sha256, s.sha256);
    assert_eq!(r.source.rev, NOW);
}

#[test]
fn only_the_first_or_only_the_last_line_found_still_gives_a_place() {
    let then = file(40);
    let s = reference("a.rs", 10, 14, &then);
    // The last line went.
    let mut now = then.clone();
    now[13] = "    let a_different_last_line = 1;".to_string();
    now[11] = "    let a_different_middle_line = 1;".to_string();
    let r = go(&s, &Files::default().with("a.rs", &now));
    assert_eq!((r.status, r.to), (RefStatus::Changed, (10, 14)), "from the first line, as long as it was");
    // The first line went.
    let mut now = then.clone();
    now[9] = "    let a_different_first_line = 1;".to_string();
    now[11] = "    let a_different_middle_line = 1;".to_string();
    let r = go(&s, &Files::default().with("a.rs", &now));
    assert_eq!((r.status, r.to), (RefStatus::Changed, (10, 14)), "back from the last line");
}

#[test]
fn lines_that_say_nothing_are_not_what_a_reference_is_found_by() {
    // A block that is all braces and punctuation: found as a block or not at all, never by a line that is everywhere.
    let then: Vec<String> = ["fn a() {", "    x();", "}", "", "}", "});", "}"].iter().map(|s| (*s).to_string()).collect();
    let s = reference("a.rs", 5, 7, &then);
    let now: Vec<String> = ["fn b() {", "    y();", "}", "}", "}", "}"].iter().map(|s| (*s).to_string()).collect();
    let r = go(&s, &Files::default().with("a.rs", &now));
    assert_eq!(r.status, RefStatus::Vanished, "no line of it says anything to look for");
    assert!(!distinctive("}") && !distinctive("});") && !distinctive("// ---") && distinctive("fn alpha() {") && distinctive("x1y2"));
}

#[test]
fn a_file_that_is_gone_or_has_nothing_of_the_lines_is_vanished_and_left_as_it_was() {
    let then = file(40);
    let s = reference("a.rs", 10, 14, &then);
    let r = go(&s, &Files::default());
    assert_eq!((r.status, r.to), (RefStatus::Vanished, (10, 14)));
    assert_eq!(r.source, s, "a vanished reference is as it was: its revision is the old one");
    let other: Vec<String> = (0..30).map(|i| format!("    unrelated_{i}();")).collect();
    let r = go(&s, &Files::default().with("a.rs", &other));
    assert_eq!(r.status, RefStatus::Vanished);
    assert_eq!(r.source.rev, THEN);
}

#[test]
fn the_new_place_is_kept_inside_the_file_and_a_foreign_reference_with_no_hashes_is_found_by_its_lines() {
    let then = file(40);
    let mut s = reference("a.rs", 36, 40, &then);
    let now = file(38);
    let r = go(&s, &Files::default().with("a.rs", &now));
    assert!(r.to.1 <= 38, "past the end of the file there is nothing: {:?}", r.to);
    // A reference from another tool has no file hash and no revision: its lines find it.
    s.file_hash = 0;
    s.range_hash = lines_hash(&s.lines);
    s.rev = String::new();
    s.sha256 = String::new();
    let r = relocate(&s, &Files::default().with("a.rs", &then), None, &LIMITS);
    assert_eq!(r.status, RefStatus::Unchanged);
    assert_eq!(r.source.rev, "", "no revision is made up");
    let mut shifted = vec!["// new".to_string()];
    shifted.extend(then);
    assert_eq!(relocate(&s, &Files::default().with("a.rs", &shifted), Some("not a hash"), &LIMITS).status, RefStatus::Moved);
}

#[test]
fn the_names_of_the_statuses_are_what_a_document_writes() {
    for (s, n) in [(RefStatus::Unchanged, "unchanged"), (RefStatus::Moved, "moved"), (RefStatus::Changed, "changed"), (RefStatus::Vanished, "vanished")] {
        assert_eq!(s.name(), n);
        assert_eq!(RefStatus::from_name(n), Some(s));
    }
    assert_eq!(RefStatus::from_name("gone"), None);
    assert!(!RefStatus::Unchanged.is_flag() && RefStatus::Moved.is_flag() && RefStatus::Changed.is_flag() && RefStatus::Vanished.is_flag());
}

#[test]
fn a_big_file_is_searched_in_a_blink() {
    let then = file(50_000);
    let s = reference("big.rs", 25_000, 25_020, &then);
    let mut now = vec!["// one line above".to_string()];
    now.extend(then);
    let files = Files::default().with("big.rs", &now);
    let began = std::time::Instant::now();
    let r = go(&s, &files);
    assert_eq!((r.status, r.to), (RefStatus::Moved, (25_001, 25_021)));
    assert!(began.elapsed().as_millis() < 500, "{:?}", began.elapsed());
}
