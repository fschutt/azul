//! Tests of the compositor's damage helpers: the cut of damage to the frame, and the order the
//! blits of a layout move run in.

use super::*;

fn rect(x: f32, y: f32, width: f32, height: f32) -> LogicalRect {
    LogicalRect {
        origin: LogicalPosition { x, y },
        size: LogicalSize { width, height },
    }
}

// ============================== clip_damage_to_frame ====================

#[test]
fn damage_inside_the_frame_is_kept_as_it_is() {
    let mut damage = vec![rect(8.0, 8.0, 100.0, 30.0), rect(0.0, 0.0, 500.0, 600.0)];
    let before = damage.clone();
    clip_damage_to_frame(&mut damage, 500.0, 600.0);
    assert_eq!(damage, before);
}

#[test]
fn damage_past_the_edge_is_cut_at_it() {
    // A bar 784 px wide in a window of 800 that shrinks to 500: the strip it lost lies from
    // 492 to 792, and of it the frame holds the 8 px up to its edge.
    let mut damage = vec![rect(492.0, 8.0, 300.0, 30.0)];
    clip_damage_to_frame(&mut damage, 500.0, 600.0);
    assert_eq!(damage, vec![rect(492.0, 8.0, 8.0, 30.0)]);
    // The same past the bottom edge, and past both.
    let mut damage = vec![rect(10.0, 590.0, 40.0, 40.0), rect(490.0, 590.0, 40.0, 40.0)];
    clip_damage_to_frame(&mut damage, 500.0, 600.0);
    assert_eq!(
        damage,
        vec![rect(10.0, 590.0, 40.0, 10.0), rect(490.0, 590.0, 10.0, 10.0)]
    );
}

#[test]
fn damage_with_no_pixel_in_the_frame_is_dropped() {
    let mut damage = vec![
        rect(600.0, 8.0, 200.0, 30.0), // wholly past the right edge
        rect(8.0, 700.0, 50.0, 50.0),  // wholly past the bottom edge
        rect(500.0, 0.0, 10.0, 10.0),  // touches the edge, holds no pixel
        rect(20.0, 20.0, 0.0, 10.0),   // no width
        rect(10.0, 10.0, 5.0, 5.0),    // inside: the one that stays
    ];
    clip_damage_to_frame(&mut damage, 500.0, 600.0);
    assert_eq!(damage, vec![rect(10.0, 10.0, 5.0, 5.0)]);
}

#[test]
fn damage_that_starts_before_the_frame_is_cut_at_its_origin() {
    // The strip a blit leaves is inflated by a pixel: at the frame's top left it starts at -1.
    let mut damage = vec![rect(-1.0, -1.0, 242.0, 42.0)];
    clip_damage_to_frame(&mut damage, 320.0, 120.0);
    assert_eq!(damage, vec![rect(0.0, 0.0, 241.0, 41.0)]);
}

// ============================== blit_order ==============================

#[test]
fn movers_sliding_over_the_next_one_are_blitted_far_end_first() {
    // `justify-content: center` slides two items at 0..60 and 60..120 by 90: the first one's
    // destination (90..150) covers the second one's source, so the second goes first.
    let movers = [rect(0.0, 0.0, 60.0, 40.0), rect(60.0, 0.0, 60.0, 40.0)];
    assert_eq!(blit_order(&movers, (90.0, 0.0), 1.0), vec![1, 0]);
    // The other way along the row, the first goes first.
    assert_eq!(blit_order(&movers, (-90.0, 0.0), 1.0), vec![0, 1]);
    // Down a column: the same rule on the other axis.
    let column = [rect(0.0, 0.0, 40.0, 60.0), rect(0.0, 60.0, 40.0, 60.0)];
    assert_eq!(blit_order(&column, (0.0, 90.0), 1.0), vec![1, 0]);
    assert_eq!(blit_order(&column, (0.0, -90.0), 1.0), vec![0, 1]);
    // Along a diagonal.
    assert_eq!(blit_order(&movers, (90.0, 10.0), 1.0), vec![1, 0]);
}

#[test]
fn movers_that_do_not_reach_one_another_keep_their_list_order() {
    // By 180 the first item (0..60) lands at 180..240, clear of the second (60..120)'s source.
    let row = [rect(0.0, 0.0, 60.0, 40.0), rect(60.0, 0.0, 60.0, 40.0)];
    assert_eq!(blit_order(&row, (180.0, 0.0), 1.0), vec![0, 1]);
    // Far apart, or on different rows: nothing constrains the order.
    let apart = [
        rect(0.0, 0.0, 10.0, 10.0),
        rect(500.0, 0.0, 10.0, 10.0),
        rect(0.0, 300.0, 10.0, 10.0),
    ];
    assert_eq!(blit_order(&apart, (5.0, 0.0), 1.0), vec![0, 1, 2]);
    assert_eq!(blit_order(&[], (5.0, 0.0), 1.0), Vec::<usize>::new());
}

#[test]
fn a_chain_of_movers_is_blitted_from_its_far_end() {
    // Three items in a row moved by 70: each one's destination reaches the next one's source.
    let movers = [
        rect(0.0, 0.0, 60.0, 40.0),
        rect(60.0, 0.0, 60.0, 40.0),
        rect(120.0, 0.0, 60.0, 40.0),
    ];
    assert_eq!(blit_order(&movers, (70.0, 0.0), 1.0), vec![2, 1, 0]);
    // List order does not matter: the one at the far end goes first.
    let shuffled = [movers[1], movers[0], movers[2]];
    assert_eq!(blit_order(&shuffled, (70.0, 0.0), 1.0), vec![2, 0, 1]);
}

// ============================== execute_translate_blit ==================

const RED: [u8; 4] = [0xcc, 0x22, 0x22, 0xff];
const BLUE: [u8; 4] = [0x22, 0x55, 0xcc, 0xff];

fn pixel(pixmap: &AzulPixmap, x: u32, y: u32) -> [u8; 4] {
    let at = ((y * pixmap.width() + x) * 4) as usize;
    let data = pixmap.data();
    [data[at], data[at + 1], data[at + 2], data[at + 3]]
}

/// A 320 x 120 white frame with a red item at 0..60 and a blue one at 60..120, 40 px tall.
fn two_items() -> AzulPixmap {
    let mut frame = AzulPixmap::new(320, 120).expect("a pixmap");
    frame.fill(255, 255, 255, 255);
    frame.fill_rect(0, 0, 60, 40, RED[0], RED[1], RED[2], RED[3]);
    frame.fill_rect(60, 0, 60, 40, BLUE[0], BLUE[1], BLUE[2], BLUE[3]);
    frame
}

fn blit(frame: &mut AzulPixmap, movers: &[LogicalRect], delta: (f32, f32)) {
    let hint = TranslateHint {
        delta,
        region_old: movers[0],
    };
    let _ = execute_translate_blit(frame, &hint, &[], movers, &[], &DisplayList::default(), 1.0);
}

#[test]
fn a_blit_does_not_carry_the_pixels_of_a_mover_over_the_one_after_it() {
    // The frame `justify-content: center` leaves: both items 90 px further along. The first
    // item's destination (90..150) covers the second one's source (60..120) from 90 on; blitted in
    // list order, the second arrived at 150..210 holding the first one's red from 180 on.
    let mut frame = two_items();
    let movers = [rect(0.0, 0.0, 60.0, 40.0), rect(60.0, 0.0, 60.0, 40.0)];
    blit(&mut frame, &movers, (90.0, 0.0));
    for x in [95, 120, 149] {
        assert_eq!(pixel(&frame, x, 10), RED, "the red item at {x}");
    }
    for x in [150, 165, 179, 180, 195, 209] {
        assert_eq!(pixel(&frame, x, 10), BLUE, "the blue item at {x}");
    }
}

#[test]
fn a_blit_carries_each_mover_whatever_order_the_list_names_them_in() {
    let mut frame = two_items();
    let movers = [rect(60.0, 0.0, 60.0, 40.0), rect(0.0, 0.0, 60.0, 40.0)];
    blit(&mut frame, &movers, (90.0, 0.0));
    assert_eq!(pixel(&frame, 100, 10), RED);
    assert_eq!(pixel(&frame, 200, 10), BLUE);
    // And in the other direction: both items 40 px to the left. The blue one's destination
    // (20..80) covers the red one's source (0..60), so the red one is carried first: what is
    // left of it on screen (0..20) is its own right part, 40..60.
    let mut frame = two_items();
    blit(&mut frame, &movers, (-40.0, 0.0));
    assert_eq!(pixel(&frame, 10, 10), RED, "the red item, what is left of it");
    assert_eq!(pixel(&frame, 25, 10), BLUE, "the blue item, 20..80");
    assert_eq!(pixel(&frame, 70, 10), BLUE);
}
