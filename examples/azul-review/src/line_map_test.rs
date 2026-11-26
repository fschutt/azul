//! Where the lines of a page are when lines are put in between them, and where a point of the page goes and comes from. Pure:
//! numbers in, numbers out.

use super::*;

const PAD: f32 = 32.0;
const H: f32 = 15.0;

fn map(count: usize, blocks: &[(Option<usize>, usize)]) -> LineMap {
    let blocks: Vec<Block> = blocks
        .iter()
        .enumerate()
        .map(|(i, (after, rows))| Block { after: *after, lines: vec!["x".to_string(); *rows], accepted: false, conversation: i as u64 + 1 })
        .collect();
    LineMap::new(count, blocks, PAD, H)
}

fn insert(after_line: usize, rows: usize, conversation: u64) -> Insert {
    Insert { after_line, rows: (0..rows).map(|i| format!("row {i}")).collect(), accepted: false, conversation }
}

#[test]
fn a_page_with_nothing_put_in_is_its_own_map() {
    let m = map(46, &[]);
    assert!(m.is_plain());
    assert_eq!(m.extra_rows(), 0);
    for y in [0.0, 10.0, PAD, 100.5, 500.0, 721.9, 800.0] {
        assert_eq!(m.to_display(y), y);
        assert_eq!(m.to_original(y), y);
    }
    assert_eq!(m.grown_height(730.0), 730.0);
    assert_eq!(m.line_top(0), PAD);
    assert_eq!(m.line_top(10), PAD + 10.0 * H);
}

#[test]
fn lines_put_in_push_the_lines_after_them_down_and_nothing_above() {
    // two lines after the fourth line of the page (index 3)
    let m = map(46, &[(Some(3), 2)]);
    assert!(!m.is_plain());
    assert_eq!(m.extra_rows(), 2);
    assert_eq!(m.line_top(3), PAD + 3.0 * H, "the fourth line stays");
    assert_eq!(m.block_top(0), PAD + 4.0 * H, "the block begins where the line after the fourth would have");
    assert_eq!(m.line_top(4), PAD + 6.0 * H, "the fifth line is two rows lower");
    assert_eq!(m.line_top(45), PAD + 47.0 * H);
    // a point on a line moves with it, the position in the line kept
    assert_eq!(m.to_display(PAD + 3.0 * H + 4.0), PAD + 3.0 * H + 4.0);
    assert_eq!(m.to_display(PAD + 4.0 * H + 4.0), PAD + 6.0 * H + 4.0);
    assert_eq!(m.grown_height(730.0), 730.0 + 2.0 * H, "the page grows, nothing else does");
}

#[test]
fn a_point_goes_to_where_it_is_drawn_and_comes_back_to_where_it_was() {
    let m = map(46, &[(Some(3), 2), (Some(10), 1), (None, 1)]);
    // every point on a line of the page, a quarter of the way down it and halfway
    for line in 0..46 {
        for frac in [0.0_f32, 4.0, 7.5, 14.5] {
            let y = PAD + line as f32 * H + frac;
            let shown = m.to_display(y);
            assert!((m.to_original(shown) - y).abs() < 1e-3, "line {line} +{frac}: {y} -> {shown} -> {}", m.to_original(shown));
            assert!(shown >= y, "a line is never higher than it was");
        }
    }
}

#[test]
fn a_point_drawn_over_the_lines_put_in_is_the_edge_of_the_lines_it_was_drawn_between() {
    let m = map(46, &[(Some(3), 2)]);
    // display rows 4 and 5 are the block: a point in either is the top of the fifth line as the page was
    let top_of_fifth = PAD + 4.0 * H;
    assert_eq!(m.to_original(PAD + 4.0 * H + 1.0), top_of_fifth);
    assert_eq!(m.to_original(PAD + 5.0 * H + 14.0), top_of_fifth);
    // and the first row after the block is the fifth line
    assert_eq!(m.to_original(PAD + 6.0 * H + 2.0), top_of_fifth + 2.0);
}

#[test]
fn lines_put_in_before_the_first_line_move_every_line() {
    let m = map(46, &[(None, 3)]);
    assert_eq!(m.block_top(0), PAD);
    assert_eq!(m.line_top(0), PAD + 3.0 * H);
    assert_eq!(m.to_display(PAD + 2.0), PAD + 3.0 * H + 2.0);
    assert_eq!(m.to_display(PAD - 10.0), PAD - 10.0, "the padding above the code is not a line");
    assert_eq!(m.to_original(PAD + 1.0), PAD, "a point over the block is the top of the first line");
}

#[test]
fn lines_put_in_after_the_last_line_grow_the_foot_of_the_page() {
    let m = map(46, &[(Some(45), 4)]);
    assert_eq!(m.line_top(45), PAD + 45.0 * H);
    assert_eq!(m.block_top(0), PAD + 46.0 * H);
    assert_eq!(m.to_display(PAD + 46.0 * H + 5.0), PAD + 50.0 * H + 5.0, "below the code everything moves by the rows put in");
    assert_eq!(m.to_original(PAD + 50.0 * H + 5.0), PAD + 46.0 * H + 5.0);
    assert_eq!(m.grown_height(730.0), 730.0 + 4.0 * H);
}

#[test]
fn blocks_after_the_same_line_keep_the_order_they_were_given() {
    let m = map(10, &[(Some(2), 1), (Some(2), 2), (Some(5), 1)]);
    assert_eq!(m.block_top(0), PAD + 3.0 * H);
    assert_eq!(m.block_top(1), PAD + 4.0 * H);
    assert_eq!(m.line_top(3), PAD + 6.0 * H);
    assert_eq!(m.block_top(2), PAD + 9.0 * H, "after the sixth line: six lines and the three rows put in before it");
    assert_eq!(m.extra_rows(), 4);
}

#[test]
fn a_block_after_a_line_the_page_does_not_have_is_after_the_last() {
    let m = map(10, &[(Some(99), 2)]);
    assert_eq!(m.block_top(0), PAD + 10.0 * H);
    assert_eq!(m.extra_rows(), 2);
}

#[test]
fn what_a_hostile_archive_can_put_in_is_bounded_and_said() {
    let blocks: Vec<Block> = (0..40).map(|i| Block { after: Some(i % 10), lines: vec!["x".to_string(); 50], accepted: false, conversation: 1 }).collect();
    let (m, dropped) = LineMap::capped(10, blocks, PAD, H);
    assert!(m.extra_rows() <= MAX_EXTRA_ROWS, "{}", m.extra_rows());
    assert!(dropped > 0, "the blocks that did not fit are counted");
    let (none, dropped) = LineMap::capped(10, Vec::new(), PAD, H);
    assert!(none.is_plain() && dropped == 0);
}

#[test]
fn a_number_that_is_no_number_is_never_a_panic() {
    let m = map(46, &[(Some(3), 2)]);
    assert!(m.to_display(f32::NAN).is_nan());
    assert_eq!(m.to_display(f32::INFINITY), f32::INFINITY);
    assert!(m.to_original(f32::NAN).is_nan());
    assert_eq!(m.to_original(f32::NEG_INFINITY), f32::NEG_INFINITY);
    assert_eq!(m.to_display(-5.0), -5.0);
    assert_eq!(map(0, &[]).to_display(100.0), 100.0, "a page with no lines");
    let empty = map(0, &[(Some(0), 2)]);
    assert!(empty.extra_rows() <= 2);
    let _ = empty.to_display(50.0);
    let _ = empty.to_original(50.0);
}

#[test]
fn the_inserts_of_a_page_are_its_blocks_after_the_line_of_the_page_they_follow() {
    // the page begins at line 47 of the file and has 46 lines
    let inserts = vec![insert(47, 2, 7), insert(60, 1, 8), insert(92, 3, 9)];
    let m = LineMap::from_inserts(47, 46, &inserts, PAD, H);
    assert_eq!(m.extra_rows(), 6);
    assert_eq!(m.block_top(0), PAD + 1.0 * H, "after line 47, the first line of the page");
    assert_eq!(m.block_top(1), PAD + (14.0 + 2.0) * H, "after line 60: fourteen lines of the page and the first block's rows");
    assert_eq!(m.block_top(2), PAD + (46.0 + 3.0) * H, "after line 92, the last line of the page: 46 lines and the three rows put in before it");
    // before the first line of the file: the first page only
    let first = LineMap::from_inserts(1, 46, &[insert(0, 2, 3)], PAD, H);
    assert_eq!(first.block_top(0), PAD);
    assert_eq!(first.line_top(0), PAD + 2.0 * H);
}

#[test]
fn the_conversation_of_a_block_is_known_so_that_the_page_can_say_whose_lines_they_are() {
    let m = LineMap::from_inserts(1, 46, &[insert(4, 2, 7), insert(10, 1, 8)], PAD, H);
    assert_eq!(m.blocks().iter().map(|b| (b.rows(), b.conversation)).collect::<Vec<_>>(), vec![(2, 7), (1, 8)]);
    // and what they say, and whether they were taken
    assert_eq!(m.blocks()[0].lines, vec!["row 0".to_string(), "row 1".to_string()]);
    let taken = Insert { accepted: true, ..insert(2, 1, 9) };
    assert!(LineMap::from_inserts(1, 46, &[taken], PAD, H).blocks()[0].accepted);
}

#[test]
fn the_page_draws_a_line_and_then_the_blocks_that_follow_it() {
    let m = LineMap::from_inserts(1, 10, &[insert(0, 1, 1), insert(3, 2, 2), insert(3, 1, 3), insert(10, 1, 4), insert(99, 1, 5)], PAD, H);
    let after = |line: Option<usize>| m.blocks_after(line).map(|b| b.conversation).collect::<Vec<_>>();
    assert_eq!(after(None), vec![1], "before the first line");
    assert_eq!(after(Some(2)), vec![2, 3], "after the third line, in the order they were given");
    assert_eq!(after(Some(9)), vec![4, 5], "after the last line: the one that follows it and the one past it");
    assert!(after(Some(0)).is_empty() && after(Some(5)).is_empty());
    // every block is drawn exactly once
    let drawn: usize = std::iter::once(None).chain((0..10).map(Some)).map(|l| m.blocks_after(l).count()).sum();
    assert_eq!(drawn, m.blocks().len());
}

#[test]
fn maps_that_draw_the_same_have_the_same_signature_and_others_do_not() {
    assert_eq!(map(46, &[]).signature(), 0, "a plain page");
    let a = map(46, &[(Some(3), 2)]);
    assert_ne!(a.signature(), 0);
    assert_eq!(a.signature(), map(46, &[(Some(3), 2)]).signature());
    assert_ne!(a.signature(), map(46, &[(Some(3), 1)]).signature(), "fewer rows");
    assert_ne!(a.signature(), map(46, &[(Some(4), 2)]).signature(), "after another line");
    assert_ne!(a.signature(), map(46, &[(None, 2)]).signature(), "before the first");
    assert_ne!(a.signature(), map(46, &[(Some(3), 1), (Some(3), 1)]).signature(), "two blocks are not one");
}

#[test]
fn the_plain_map_that_asks_for_no_page_is_every_point_where_it_was() {
    let m = LineMap::plain();
    assert!(m.is_plain());
    for y in [-3.0, 0.0, 32.0, 100.0, 5_000.0] {
        assert_eq!(m.to_display(y), y);
        assert_eq!(m.to_original(y), y);
    }
    assert_eq!(m.signature(), 0);
    assert_eq!(m.grown_height(730.0), 730.0);
}
