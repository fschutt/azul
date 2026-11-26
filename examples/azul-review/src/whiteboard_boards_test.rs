//! The boards of a workspace: the list, its order, the one that is open, the names, the caps, the soft delete and
//! its undo, the index file and what is made of a damaged or hostile one. No window, no azul.

use super::*;
use crate::whiteboard_store::board_name;

const W: &str = "/work/azul";

fn index() -> BoardIndex {
    BoardIndex::fresh(W, "Board 1", 100)
}

fn names(i: &BoardIndex) -> Vec<String> {
    i.boards().map(|b| b.name.clone()).collect()
}

fn ids(i: &BoardIndex) -> Vec<String> {
    i.boards().map(|b| b.id.clone()).collect()
}

#[test]
fn a_new_workspace_has_one_board_in_a_file_named_for_its_id_and_an_old_single_file_becomes_board_one_in_place() {
    let fresh = index();
    assert_eq!(ids(&fresh), vec!["b1"]);
    assert_eq!(fresh.active().id, "b1");
    assert!(fresh.active().file.ends_with(".b1.azboard.zip"), "{}", fresh.active().file);
    assert!(fresh.active().file.starts_with("boards/azul-"), "{}", fresh.active().file);
    assert_eq!((fresh.active().created_ms, fresh.active().modified_ms), (100, 100));
    assert_eq!(fresh.next_number(), 2);
    // The file of the one-board version is the first board's: not renamed, not copied.
    let old = BoardIndex::migrated(W, "Board 1", 77);
    assert_eq!(old.active().file, board_name(W));
    assert_eq!((old.active().created_ms, old.active().modified_ms), (77, 77));
    assert_eq!(names(&old), vec!["Board 1"]);
    // The next board's file is its own.
    let mut old = old;
    let (id, _) = old.create("Ideas", 80).expect("created");
    assert_eq!(old.get(&id).map(|b| b.file.clone()), Some(format!("{}.{id}.azboard.zip", board_name(W).trim_end_matches(".azboard.zip"))));
}

#[test]
fn boards_come_in_the_order_made_and_an_id_is_never_given_twice_even_after_a_delete() {
    let mut i = index();
    let (b2, cut) = i.create("Design", 110).expect("created");
    assert_eq!((b2.as_str(), cut), ("b2", false));
    let (b3, _) = i.create("Ideas", 120).expect("created");
    assert_eq!(names(&i), vec!["Board 1", "Design", "Ideas"]);
    i.delete(&b3, 130, "Board 4").expect("deleted");
    let (b4, _) = i.create("More", 140).expect("created");
    assert_eq!(b4, "b4", "b3 is not given again, deleted or not");
    assert_eq!(i.next_number(), 5);
    let mut all: Vec<_> = i.all().map(|b| b.id.clone()).collect();
    all.sort();
    all.dedup();
    assert_eq!(all.len(), 4, "every id once");
}

#[test]
fn a_name_is_trimmed_made_one_line_cut_at_the_cap_and_never_empty() {
    let mut i = index();
    let (id, cut) = i.create("  Parser\nand lexer\t ", 1).expect("created");
    assert_eq!((i.get(&id).map(|b| b.name.as_str()), cut), (Some("Parser and lexer"), false));
    let long = "x".repeat(MAX_NAME_CHARS + 30);
    let (long_id, cut) = i.create(&long, 2).expect("created");
    assert!(cut, "the cap is told");
    assert_eq!(i.get(&long_id).map(|b| b.name.chars().count()), Some(MAX_NAME_CHARS));
    assert_eq!(i.create("   \n ", 3), Err(Notice::BoardNameEmpty));
    assert_eq!(i.rename(&id, "", 4), Err(Notice::BoardNameEmpty));
    assert_eq!(i.rename(&id, "Renamed", 5), Ok(false));
    assert_eq!(i.get(&id).map(|b| (b.name.as_str(), b.modified_ms)), Some(("Renamed", 5)));
    assert_eq!(i.rename("nope", "x", 6), Err(Notice::NoSuchBoard));
    // Letters of any script count as one each: the cap is in characters, not bytes.
    let wide = "板".repeat(MAX_NAME_CHARS);
    assert_eq!(i.rename(&id, &wide, 7), Ok(false));
}

#[test]
fn a_workspace_holds_so_many_boards_and_no_more_and_the_deleted_ones_do_not_count() {
    let mut i = index();
    for n in 2..=MAX_BOARDS {
        i.create(&format!("Board {n}"), n as u64).expect("room");
    }
    assert_eq!(i.boards().count(), MAX_BOARDS);
    assert_eq!(i.create("one too many", 999), Err(Notice::TooManyBoards { limit: MAX_BOARDS }));
    i.delete("b7", 1_000, "fresh").expect("deleted");
    assert_eq!(i.boards().count(), MAX_BOARDS - 1);
    assert!(i.create("room again", 1_001).is_ok());
    // A deleted board comes back only while there is room for it.
    assert_eq!(i.restore("b7"), Err(Notice::TooManyBoards { limit: MAX_BOARDS }));
}

#[test]
fn deleting_is_soft_the_open_board_moves_to_a_neighbour_and_restoring_puts_it_back_where_it_was() {
    let mut i = index();
    i.create("Two", 2).expect("created");
    i.create("Three", 3).expect("created");
    i.set_active("b2").expect("active");
    let gone = i.delete("b2", 50, "Board 4").expect("deleted");
    assert_eq!((gone.replaced_by_new, gone.now_active.as_str()), (false, "b3"), "the next one takes over");
    assert_eq!(ids(&i), vec!["b1", "b3"]);
    assert_eq!(i.get("b2").and_then(|b| b.deleted_ms), Some(50), "it is still there, marked");
    i.restore("b2").expect("restored");
    assert_eq!(ids(&i), vec!["b1", "b2", "b3"], "back in its place");
    assert_eq!(i.active().id, "b3", "the open board stays the one it was");
    // The last in the list: the one before it takes over.
    i.set_active("b3").expect("active");
    let gone = i.delete("b3", 60, "x").expect("deleted");
    assert_eq!(gone.now_active, "b2");
    // A board that is not open: the open one stays.
    let gone = i.delete("b1", 61, "x").expect("deleted");
    assert_eq!(gone.now_active, "b2");
    // The last board of all: a fresh one takes its place, with the name given.
    let gone = i.delete("b2", 62, "Board 5").expect("deleted");
    assert!(gone.replaced_by_new);
    assert_eq!(names(&i), vec!["Board 5"]);
    assert_eq!(i.active().id, gone.now_active);
    assert_eq!(i.delete("nope", 63, "x").err(), Some(Notice::NoSuchBoard));
    assert_eq!(i.restore("nope"), Err(Notice::NoSuchBoard));
}

#[test]
fn a_board_moves_in_the_list_among_the_boards_that_are_shown() {
    let mut i = index();
    for n in ["Two", "Three", "Four"] {
        i.create(n, 2).expect("created");
    }
    i.delete("b3", 9, "x").expect("deleted");
    assert_eq!(ids(&i), vec!["b1", "b2", "b4"]);
    i.move_to("b4", 0).expect("moved");
    assert_eq!(ids(&i), vec!["b4", "b1", "b2"]);
    i.move_to("b4", 99).expect("moved to the end");
    assert_eq!(ids(&i), vec!["b1", "b2", "b4"]);
    i.move_to("b2", 0).expect("moved");
    assert_eq!(ids(&i), vec!["b2", "b1", "b4"]);
    // The deleted one keeps its place relative to the others: restored, it is where it was among them.
    i.restore("b3").expect("restored");
    assert!(ids(&i).iter().position(|b| b == "b3").is_some());
    assert_eq!(i.move_to("nope", 0), Err(Notice::NoSuchBoard));
    assert_eq!(i.get("b2").map(|b| b.modified_ms), Some(2), "a move is a change of the list, not of the board: it keeps the time it had");
}

#[test]
fn only_a_board_that_is_shown_can_be_opened() {
    let mut i = index();
    i.create("Two", 2).expect("created");
    i.delete("b2", 3, "x").expect("deleted");
    assert_eq!(i.set_active("b2"), Err(Notice::NoSuchBoard));
    assert_eq!(i.set_active("nope"), Err(Notice::NoSuchBoard));
    assert_eq!(i.set_active("b1"), Ok(()));
}

#[test]
fn the_index_is_written_as_json_and_read_back_the_same() {
    let mut i = BoardIndex::migrated(W, "Board 1", 5);
    i.create("Design \"v2\"", 6).expect("created");
    i.create("Ideas", 7).expect("created");
    i.delete("b3", 8, "x").expect("deleted");
    i.set_active("b2").expect("active");
    let text = i.to_json();
    assert!(text.contains("\"format\": \"azreview-boards/1\""), "{text}");
    let (back, dropped) = BoardIndex::from_json(&text, W).expect("read");
    assert_eq!(dropped, 0);
    assert_eq!(back, i);
    assert_eq!(back.get("b3").and_then(|b| b.deleted_ms), Some(8));
    assert_eq!(back.get("b2").map(|b| b.name.as_str()), Some("Design \"v2\""));
}

#[test]
fn an_index_that_is_damaged_or_of_a_newer_version_or_far_too_large_is_refused_in_words() {
    assert!(matches!(BoardIndex::from_json("", W), Err(BoardsError::NotAnIndex(_))));
    assert!(matches!(BoardIndex::from_json("{\"format\": \"azreview-boards/1\"", W), Err(BoardsError::NotAnIndex(_))));
    assert!(matches!(BoardIndex::from_json("[1, 2]", W), Err(BoardsError::NotAnIndex(_))));
    assert!(matches!(BoardIndex::from_json("{\"format\": \"something-else/1\", \"boards\": []}", W), Err(BoardsError::NotAnIndex(_))));
    assert!(matches!(BoardIndex::from_json("{\"format\": \"azreview-boards/1\", \"boards\": []}", W), Err(BoardsError::NotAnIndex(_))), "no board at all is no index");
    assert_eq!(
        BoardIndex::from_json("{\"format\": \"azreview-boards/4\", \"boards\": []}", W).err(),
        Some(BoardsError::TooNew { found: 4 })
    );
    let big = format!("{{\"format\": \"azreview-boards/1\", \"pad\": \"{}\"}}", "x".repeat(MAX_INDEX_BYTES));
    assert!(matches!(BoardIndex::from_json(&big, W), Err(BoardsError::TooLarge { .. })));
    for e in [BoardsError::NotAnIndex("a comma was expected".into()), BoardsError::TooNew { found: 4 }, BoardsError::TooLarge { bytes: 9 }] {
        assert!(e.english().len() > 20 && e.key().contains("whiteboard-boards-"), "{e:?}");
    }
}

#[test]
fn what_an_index_says_is_checked_a_file_name_that_is_not_the_boards_own_an_id_twice_too_many_boards_a_stray_active() {
    let own = board_name(W);
    let json = format!(
        "{{\"format\": \"azreview-boards/1\", \"next\": 3, \"active\": \"zzz\", \"boards\": [\
         {{\"id\": \"b1\", \"name\": \"One\", \"created_ms\": 1, \"modified_ms\": 2, \"file\": \"{own}\"}},\
         {{\"id\": \"b1\", \"name\": \"Twice\", \"created_ms\": 1, \"modified_ms\": 2, \"file\": \"{own}\"}},\
         {{\"id\": \"b2\", \"name\": \"Out\", \"created_ms\": 1, \"modified_ms\": 2, \"file\": \"../../etc/passwd\"}},\
         {{\"id\": \"b3\", \"name\": \"Elsewhere\", \"created_ms\": 1, \"modified_ms\": 2, \"file\": \"boards/other-0000000000000000.b3.azboard.zip\"}},\
         {{\"id\": \"c4\", \"name\": \"Odd id\", \"created_ms\": 1, \"modified_ms\": 2, \"file\": \"{own}\"}},\
         {{\"name\": \"No id\"}}\
         ]}}"
    );
    let (i, dropped) = BoardIndex::from_json(&json, W).expect("read");
    assert_eq!(ids(&i), vec!["b1"], "one board is left: the others are not what a board of this workspace is");
    assert_eq!(dropped, 5);
    assert_eq!(i.active().id, "b1", "an active board that is not there is the first one shown");
    assert_eq!(i.next_number(), 3, "the counter is what the file says, and never below what the boards left need");
    // Far more boards than the cap: the first ones are kept, the rest counted as left out.
    let mut many = String::from("{\"format\": \"azreview-boards/1\", \"boards\": [");
    for n in 1..=(MAX_BOARDS + 20) {
        if n > 1 {
            many.push(',');
        }
        let file = format!("{}.b{n}.azboard.zip", board_name(W).trim_end_matches(".azboard.zip"));
        many.push_str(&format!("{{\"id\": \"b{n}\", \"name\": \"B{n}\", \"created_ms\": 1, \"modified_ms\": 1, \"file\": \"{file}\"}}"));
    }
    many.push_str("]}");
    let (i, dropped) = BoardIndex::from_json(&many, W).expect("read");
    assert_eq!((i.all().count(), dropped), (MAX_BOARDS, 20));
}

#[test]
fn a_name_in_a_file_is_cleaned_as_a_name_typed_is() {
    let own = board_name(W);
    let json = format!(
        "{{\"format\": \"azreview-boards/1\", \"boards\": [{{\"id\": \"b1\", \"name\": \"  A\\nB  \", \"created_ms\": 1, \"modified_ms\": 2, \"file\": \"{own}\"}},\
         {{\"id\": \"b2\", \"name\": \"\", \"created_ms\": 1, \"modified_ms\": 2, \"file\": \"{}.b2.azboard.zip\"}}]}}",
        own.trim_end_matches(".azboard.zip")
    );
    let (i, dropped) = BoardIndex::from_json(&json, W).expect("read");
    assert_eq!(names(&i), vec!["A B", "Board 2"], "an empty name is the board's number");
    assert_eq!(dropped, 0);
}

#[test]
fn the_names_of_the_files_are_the_boards_own_and_nothing_in_an_id_can_lead_out_of_the_folder() {
    assert_eq!(index_name(W), format!("{}.azboards.json", board_name(W).trim_end_matches(".azboard.zip")));
    assert!(index_name(W).starts_with("boards/"));
    assert_eq!(file_name(W, "b12"), format!("{}.b12.azboard.zip", board_name(W).trim_end_matches(".azboard.zip")));
    assert!(valid_id("b1") && valid_id("b99999"));
    for bad in ["", "b", "b0x", "B1", "b-1", "../b1", "b1/../x", "b1 ", "c1", "b123456789012345678901234567890"] {
        assert!(!valid_id(bad), "{bad:?}");
    }
}
