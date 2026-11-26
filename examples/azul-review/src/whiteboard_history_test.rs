//! The history of a board: what two states of it differ in, the ids a chain of snapshots takes, what a person types as tags
//! and as the reason. No window, no azul.

use super::*;
use crate::{
    whiteboard::{Arrow, Cutout, End, ImageSlot, Source},
    whiteboard_geom::{Rect, View},
};

fn cutout(id: u64, x: f32, y: f32, label: &str, first: u32) -> Item {
    Item::Cutout(Cutout {
        id,
        rect: Rect::new(x, y, 200.0, 100.0),
        image: ImageSlot::Failed("none".to_string()),
        label: label.to_string(),
        source: Source {
            rev: String::new(),
            sha256: String::new(),
            file: "src/lib.rs".to_string(),
            page: 0,
            first_line: first,
            last_line: first + 5,
            shape: Vec::new(),
            file_hash: 0,
            range_hash: 0,
            lines: Vec::new(),
        },
    })
}

fn arrow(id: u64, from: u64, to: u64, label: &str, bend: f32) -> Item {
    Item::Arrow(Arrow { id, from: End::item(from), to: End::item(to), bend, label: label.to_string() })
}

fn board(items: Vec<Item>) -> BoardData {
    BoardData { items, view: View::default(), next_id: 100 }
}

#[test]
fn the_first_snapshot_is_all_new_and_the_same_board_is_no_change() {
    let b = board(vec![cutout(1, 0.0, 0.0, "the parser", 10), cutout(2, 300.0, 0.0, "", 40), arrow(3, 1, 2, "", 0.0)]);
    let first = diff(None, &b);
    assert_eq!(first.counts(), (3, 0, 0, 0));
    assert_eq!(first.added.iter().map(|a| (a.id, a.kind, a.name.as_str())).collect::<Vec<_>>(), vec![(1, "cutout", "the parser"), (2, "cutout", "src/lib.rs:40-45"), (3, "arrow", "the parser -> src/lib.rs:40-45")]);
    assert!(diff(Some(&b), &b).is_empty());
    assert!(diff(Some(&BoardData::default()), &BoardData::default()).is_empty());
}

#[test]
fn what_was_added_removed_moved_and_changed_is_told_by_id_in_id_order() {
    let before = board(vec![cutout(1, 0.0, 0.0, "a", 10), cutout(2, 300.0, 0.0, "b", 20), cutout(3, 600.0, 0.0, "c", 30), arrow(4, 1, 2, "calls", 0.1)]);
    let after = board(vec![
        cutout(1, 50.0, 10.0, "a", 10),            // moved
        cutout(2, 300.0, 0.0, "b renamed", 21),    // label and reference changed
        cutout(5, 900.0, 0.0, "new", 50),          // added
        arrow(4, 1, 5, "calls", 0.1),              // ends changed
        arrow(6, 2, 5, "", 0.0),                   // added
    ]);
    let s = diff(Some(&before), &after);
    assert_eq!(s.added.iter().map(|b| b.id).collect::<Vec<_>>(), vec![5, 6]);
    assert_eq!(s.removed.iter().map(|b| (b.id, b.name.as_str())).collect::<Vec<_>>(), vec![(3, "c")]);
    assert_eq!(s.moved, vec![Moved { id: 1, from: Pt::new(0.0, 0.0), to: Pt::new(50.0, 10.0) }]);
    assert_eq!(s.changed, vec![Changed { id: 2, what: vec!["label", "reference"] }, Changed { id: 4, what: vec!["ends"] }]);
    assert_eq!(s.counts(), (2, 1, 1, 2));
    assert!(!s.is_empty());
    // The same two states always give the same summary, whatever order the items are kept in.
    let mut shuffled = after.clone();
    shuffled.items.reverse();
    assert_eq!(diff(Some(&before), &shuffled), s);
}

#[test]
fn the_same_lines_at_a_newer_revision_are_the_same_reference() {
    let mut a = board(vec![cutout(1, 0.0, 0.0, "a", 10)]);
    let mut b = a.clone();
    for (data, rev) in [(&mut a, "1111111111111111111111111111111111111111"), (&mut b, "2222222222222222222222222222222222222222")] {
        if let Item::Cutout(c) = &mut data.items[0] {
            c.source.rev = rev.to_string();
            c.source.sha256 = "sha256:aaaa".to_string();
        }
    }
    assert!(diff(Some(&a), &b).is_empty(), "only the revision differs");
    if let Item::Cutout(c) = &mut b.items[0] {
        c.source.sha256 = "sha256:bbbb".to_string();
    }
    assert_eq!(diff(Some(&a), &b).changed, vec![Changed { id: 1, what: vec!["reference"] }], "the lines say something else");
}

#[test]
fn a_move_of_less_than_half_a_pixel_is_no_move_and_a_resize_or_a_bend_is_a_change() {
    let a = board(vec![cutout(1, 10.0, 10.0, "a", 1), cutout(2, 0.0, 0.0, "b", 9), arrow(3, 1, 2, "", 0.1)]);
    let mut b = a.clone();
    if let Item::Cutout(c) = &mut b.items[0] {
        c.rect.x += 0.3;
        c.rect.y -= 0.4;
    }
    assert!(diff(Some(&a), &b).is_empty(), "sub-pixel noise from a drag is not a change");
    if let Item::Cutout(c) = &mut b.items[1] {
        c.rect.w += 40.0;
    }
    if let Item::Arrow(r) = &mut b.items[2] {
        r.bend = 0.4;
    }
    let s = diff(Some(&a), &b);
    assert_eq!(s.changed, vec![Changed { id: 2, what: vec!["size"] }, Changed { id: 3, what: vec!["bend"] }]);
    assert!(s.moved.is_empty());
}

#[test]
fn an_item_whose_kind_changed_under_its_id_is_removed_and_added() {
    let a = board(vec![cutout(1, 0.0, 0.0, "a", 1)]);
    let b = board(vec![arrow(1, 2, 3, "x", 0.0)]);
    let s = diff(Some(&a), &b);
    assert_eq!((s.removed.len(), s.added.len()), (1, 1));
}

#[test]
fn a_chain_of_snapshots_takes_ids_past_every_one_it_has_and_finds_parents() {
    let snap = |id: &str, parent: Option<&str>, items: usize| Snapshot {
        id: id.to_string(),
        parent: parent.map(str::to_string),
        taken_ms: 1,
        author: None,
        code: Code::default(),
        why: "because".to_string(),
        tags: Vec::new(),
        summary: Summary::default(),
        evidence: None,
        data: board((0..items).map(|i| cutout(i as u64 + 1, 0.0, 0.0, "x", 1)).collect()),
        value: crate::whiteboard_json::Json::Null,
        hash: String::new(),
        verified: true,
        pictures: BTreeMap::new(),
    };
    let mut h = History::default();
    assert_eq!(h.next_id(), "s1");
    assert!(h.is_empty() && h.latest().is_none());
    h.snapshots.push(snap("s1", None, 2));
    h.snapshots.push(snap("s2", Some("s1"), 3));
    h.snapshots.push(snap("imported", Some("s2"), 1));
    h.snapshots.push(snap("s7", Some("imported"), 1));
    assert_eq!(h.next_id(), "s8", "past the largest number, not the count");
    assert_eq!(h.get("s2").map(|s| s.items_len()), Some(3));
    assert_eq!(h.parent_of("s2").map(|s| s.id.as_str()), Some("s1"));
    assert!(h.parent_of("s1").is_none());
    assert!(h.parent_of("nope").is_none());
    assert_eq!(h.latest().map(|s| s.id.as_str()), Some("s7"));
    assert_eq!(h.items_held(), 7);
    // Room: not past the most snapshots and not past the most items.
    assert!(h.has_room_for(100));
    assert!(!h.has_room_for(MAX_HISTORY_ITEMS));
    for i in 0..MAX_SNAPSHOTS {
        h.snapshots.push(snap(&format!("s{}", 100 + i), None, 0));
    }
    assert!(!h.has_room_for(0), "the most snapshots a board keeps");
}

impl Snapshot {
    fn items_len(&self) -> usize {
        self.data.items.len()
    }
}

#[test]
fn the_headline_is_the_first_line_of_the_reason_that_says_something() {
    let mut s = Snapshot {
        id: "s1".to_string(),
        parent: None,
        taken_ms: 0,
        author: None,
        code: Code::default(),
        why: "\n  \n  The solver moved out of the window.\nIt was too big.".to_string(),
        tags: Vec::new(),
        summary: Summary::default(),
        evidence: None,
        data: BoardData::default(),
        value: crate::whiteboard_json::Json::Null,
        hash: String::new(),
        verified: true,
        pictures: BTreeMap::new(),
    };
    assert_eq!(s.headline(), "The solver moved out of the window.");
    s.why = String::new();
    assert_eq!(s.headline(), "");
}

#[test]
fn tags_are_split_trimmed_cut_and_kept_few() {
    assert_eq!(parse_tags("ADR-7, #123 ,https://x.org/pr/9;  ADR-7\n\n  decision "), vec!["ADR-7", "#123", "https://x.org/pr/9", "decision"]);
    assert!(parse_tags("").is_empty() && parse_tags(" , ;\n ").is_empty());
    let long = "x".repeat(300);
    assert_eq!(parse_tags(&long)[0].chars().count(), MAX_TAG_CHARS);
    let many: Vec<String> = (0..100).map(|i| format!("t{i}")).collect();
    assert_eq!(parse_tags(&many.join(",")).len(), MAX_TAGS);
    assert_eq!(parse_tags("a\u{0}b, \u{7}c"), vec!["ab", "c"], "no control characters");
}

#[test]
fn a_reason_keeps_its_lines_and_loses_control_characters_and_what_is_past_the_cap() {
    assert_eq!(clean_why("  Why\nthis\tway \u{0}\u{7} "), "Why\nthis\tway");
    assert_eq!(clean_why("   "), "");
    assert_eq!(clean_why(&"y".repeat(MAX_WHY_CHARS + 50)).chars().count(), MAX_WHY_CHARS);
}

#[test]
fn the_code_a_snapshot_was_taken_at_is_what_the_repository_said() {
    let facts = RepoFacts {
        rev: Some("0123456789abcdef0123456789abcdef01234567".to_string()),
        branch: Some("main".to_string()),
        remote_url: Some("https://github.com/fschutt/azul.git".to_string()),
        web_url: Some("https://github.com/fschutt/azul".to_string()),
        host: Host::GitHub,
        author: Some("Ada".to_string()),
        prefix: "examples/azul-review".to_string(),
        dirty: Some(true),
    };
    let c = Code::of(&facts);
    assert_eq!((c.rev.as_deref(), c.branch.as_deref(), c.dirty, c.host, c.prefix.as_str()), (facts.rev.as_deref(), Some("main"), Some(true), Host::GitHub, "examples/azul-review"));
    assert_eq!(c.rev_time_ms, None);
    assert_eq!(Code::of(&RepoFacts::default()), Code::default());
}

#[test]
fn a_time_is_written_as_a_date_and_the_minute_in_utc() {
    for (ms, want) in [
        (0, "1970-01-01 00:00"),
        (86_399_999, "1970-01-01 23:59"),
        (951_782_400_000, "2000-02-29 00:00"),
        (1_700_000_000_000, "2023-11-14 22:13"),
        (1_764_140_000_000, "2025-11-26 06:53"),
        (4_102_444_800_000, "2100-01-01 00:00"),
        (1_709_251_199_000, "2024-02-29 23:59"),
        (1_709_251_200_000, "2024-03-01 00:00"),
    ] {
        assert_eq!(date_utc(ms), want, "{ms}");
    }
    assert_eq!(short_rev(Some("0123456789abcdef")), "0123456");
    assert_eq!(short_rev(Some("abc")), "abc");
    assert_eq!(short_rev(None), "");
}

