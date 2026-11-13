//! A span that holds a block is broken around it (CSS 2.2 s9.2.1.1): the
//! part before the block and the part after it are fragments of the same
//! inline box. Its border and background are drawn on both, its start edge
//! only on the first part and its end edge only on the last (WPT
//! inline-box-001, split-inline-borders).
//!
//! The broken span kept no box: its parts went into anonymous blocks with
//! the text's own style, so nothing of its border was drawn and its padding
//! moved nothing.

use crate::painted::{painted, Painted, BLUE};

fn page(body: &str) -> Painted {
    painted(
        &format!(
            "<html><head><style>body {{ margin: 0; font-size: 16px; color: rgba(0, 0, 0, 0); }} \
             </style></head><body><div style=\"width: 200px\">{body}</div></body></html>"
        ),
        200,
        100,
    )
}

/// The blue pixels of row `y` as x positions.
fn blue_row(p: &Painted, y: usize) -> Vec<usize> {
    (0..p.width).filter(|&x| p.is(x, y, BLUE, 10)).collect()
}

#[test]
fn the_first_part_has_the_start_edge_and_the_last_part_the_end_edge() {
    let p = page(
        "<span style=\"border: 4px solid rgb(0, 0, 255)\">AAA<div style=\"height: 10px\"></div>\
         BBB</span>",
    );
    let (_, y0, _, y1) = p.bounds_of(BLUE, 10).expect("the broken span draws its border");
    // The two parts: their border boxes, split at the gap the block leaves.
    let rows: Vec<usize> = (y0..y1).filter(|&y| !blue_row(&p, y).is_empty()).collect();
    let gap = rows
        .windows(2)
        .find(|w| w[1] > w[0] + 1)
        .map(|w| (w[0], w[1]))
        .expect("two parts with the block between them");
    let first_middle = (y0 + gap.0) / 2;
    let last_middle = (gap.1 + y1) / 2;

    let first = blue_row(&p, first_middle);
    assert!(
        first.first() == Some(&0) && first.iter().all(|&x| x < 4),
        "the first part has its left border (x 0..4) and no right one: {first:?}"
    );
    let last = blue_row(&p, last_middle);
    assert!(
        !last.is_empty() && last.iter().all(|&x| x > 10),
        "the last part has its right border and no left one: {last:?}"
    );
}

#[test]
fn the_start_padding_moves_only_the_first_part() {
    // The text of each part paints red: where it starts says where the pen
    // was.
    let red = |body: &str| {
        let p = painted(
            &format!(
                "<html><head><style>body {{ margin: 0; font-size: 16px; }} b {{ color: red; \
                 }}</style></head><body><div style=\"width: 200px\">{body}</div></body></html>"
            ),
            200,
            100,
        );
        let (_, y0, _, y1) = p.bounds_of((255, 0, 0), 60).expect("red text");
        let mid = (y0 + y1) / 2;
        let pr = &p;
        // The leftmost red pixel above and below the middle.
        let left_of = |ys: core::ops::Range<usize>| {
            ys.flat_map(|y| (0..pr.width).filter(move |&x| pr.is(x, y, (255, 0, 0), 60)))
                .min()
                .expect("red in the band")
        };
        (left_of(y0..mid), left_of(mid..y1))
    };
    let plain = red("<span><b>I</b><div style=\"height: 20px\"></div><b>I</b></span>");
    let padded = red(
        "<span style=\"padding-left: 30px\"><b>I</b><div style=\"height: 20px\"></div><b>I</b>\
         </span>",
    );
    assert!(
        (padded.0 as i64 - plain.0 as i64 - 30).abs() <= 1,
        "the first part starts 30px in: {plain:?} -> {padded:?}"
    );
    assert!(
        (padded.1 as i64 - plain.1 as i64).abs() <= 1,
        "the last part has no start padding: {plain:?} -> {padded:?}"
    );
}
