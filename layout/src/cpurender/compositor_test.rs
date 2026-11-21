//! Tests of the compositor's damage helpers: the cut of damage to the frame.

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
