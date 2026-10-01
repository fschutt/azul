//! The CPU compositor: tracks from V1 up, the top one wins, opacity blends;
//! position, scale and crop place a layer; cross dissolves and dips to black.

use super::*;
use crate::model::{
    Edit, Effects, MediaItem, Pattern, Project, Transition, TransitionKind,
};

const W: u32 = 32;
const H: u32 = 18;

const RED: [u8; 3] = [220, 20, 20];
const BLUE: [u8; 3] = [20, 20, 220];

fn pixel(c: &Canvas, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * c.width + x) * 4) as usize;
    [c.rgba[i], c.rgba[i + 1], c.rgba[i + 2], c.rgba[i + 3]]
}

fn near(a: [u8; 4], b: [u8; 3]) -> bool {
    a.iter().zip(b.iter()).all(|(x, y)| (i32::from(*x) - i32::from(*y)).abs() <= 3) && a[3] == 255
}

/// V1: a red matte 0..20; V2: a blue matte 0..10 (or with `blue_effects`).
fn two_layers(blue_effects: Effects) -> (Project, u64) {
    let mut p = Project::create("p".into(), "t".into(), W, H, 25);
    let red = p.add_media(MediaItem::generated("red", Pattern::Matte { rgb: RED }, 100, W, H));
    let blue = p.add_media(MediaItem::generated("blue", Pattern::Matte { rgb: BLUE }, 100, W, H));
    let a = p.clip_from_media(red, 0, 20);
    let b = p.clip_from_media(blue, 0, 10);
    let idb = b.id;
    p.edit(Edit::Overwrite { track: 0, at: 0, clip: a }).expect("V1");
    p.edit(Edit::Overwrite { track: 1, at: 0, clip: b }).expect("V2");
    p.edit(Edit::SetEffects { clip: idb, effects: blue_effects }).expect("effects");
    (p, idb)
}

fn frame(p: &Project, f: i64) -> Canvas {
    compose(p, f, W, H, &mut Generated)
}

#[test]
fn a_generated_matte_fills_the_picture_with_its_colour() {
    let c = generate(&Pattern::Matte { rgb: RED }, 7, 8, 6);
    assert_eq!((c.width, c.height, c.rgba.len()), (8, 6, 8 * 6 * 4));
    assert!(c.rgba.chunks_exact(4).all(|p| p == [RED[0], RED[1], RED[2], 255]));
    let bars = generate(&Pattern::Bars, 0, 64, 36);
    assert_ne!(pixel(&bars, 2, 2), pixel(&bars, 62, 2), "bars are bars");
}

#[test]
fn the_top_track_wins_where_it_has_a_clip_and_opacity_blends_it_over_the_one_below() {
    let (p, _) = two_layers(Effects::default());
    assert!(near(pixel(&frame(&p, 5), W / 2, H / 2), BLUE), "V2 over V1");
    assert!(near(pixel(&frame(&p, 15), W / 2, H / 2), RED), "V1 alone after V2's clip");
    let (p, _) = two_layers(Effects { opacity: 0.5, ..Effects::default() });
    let mix = pixel(&frame(&p, 5), W / 2, H / 2);
    assert!(near(mix, [120, 20, 120]), "half and half: {mix:?}");
}

#[test]
fn a_hidden_track_and_a_disabled_clip_draw_nothing() {
    let (mut p, idb) = two_layers(Effects::default());
    p.edit(Edit::SetTrackHidden { track: 1, hidden: true }).expect("hide V2");
    assert!(near(pixel(&frame(&p, 5), W / 2, H / 2), RED));
    p.edit(Edit::SetTrackHidden { track: 1, hidden: false }).expect("show V2");
    p.edit(Edit::SetEnabled { clip: idb, enabled: false }).expect("disable");
    assert!(near(pixel(&frame(&p, 5), W / 2, H / 2), RED));
}

#[test]
fn frames_outside_every_clip_are_black() {
    let (p, _) = two_layers(Effects::default());
    assert!(near(pixel(&frame(&p, 25), 0, 0), [0, 0, 0]));
}

#[test]
fn scale_and_position_place_a_layer_and_crop_cuts_it() {
    // Half size, centred: blue in the middle, red in the corner.
    let (p, _) = two_layers(Effects { scale: 0.5, ..Effects::default() });
    let c = frame(&p, 5);
    assert!(near(pixel(&c, W / 2, H / 2), BLUE));
    assert!(near(pixel(&c, 1, 1), RED));
    // Moved a quarter of the width right: the middle-left is red now.
    let (p, _) = two_layers(Effects { scale: 0.5, x: W as f32 / 4.0, ..Effects::default() });
    let c = frame(&p, 5);
    assert!(near(pixel(&c, W / 2 - 3, H / 2), RED));
    assert!(near(pixel(&c, W * 3 / 4, H / 2), BLUE));
    // Full size, the left half cropped away.
    let (p, _) = two_layers(Effects { crop_left: 0.5, ..Effects::default() });
    let c = frame(&p, 5);
    assert!(near(pixel(&c, 2, H / 2), RED), "the cropped half shows V1");
    assert!(near(pixel(&c, W - 3, H / 2), BLUE));
}

/// V1: red 0..10, then blue 10..20 with `kind` over its first 4 frames.
fn transition(kind: TransitionKind) -> Project {
    let mut p = Project::create("p".into(), "t".into(), W, H, 25);
    let red = p.add_media(MediaItem::generated("red", Pattern::Matte { rgb: RED }, 100, W, H));
    let blue = p.add_media(MediaItem::generated("blue", Pattern::Matte { rgb: BLUE }, 100, W, H));
    let a = p.clip_from_media(red, 0, 10);
    let b = p.clip_from_media(blue, 0, 10);
    let idb = b.id;
    p.edit(Edit::Overwrite { track: 0, at: 0, clip: a }).expect("red");
    p.edit(Edit::Overwrite { track: 0, at: 10, clip: b }).expect("blue");
    p.edit(Edit::SetTransition { clip: idb, transition: Some(Transition { kind, frames: 4 }) })
        .expect("transition");
    p
}

#[test]
fn a_cross_dissolve_mixes_the_outgoing_and_incoming_clips_halfway_at_its_middle() {
    let p = transition(TransitionKind::CrossDissolve);
    assert!(near(pixel(&frame(&p, 10), 4, 4), RED), "it starts on the outgoing clip");
    assert!(near(pixel(&frame(&p, 12), 4, 4), [120, 20, 120]), "{:?}", pixel(&frame(&p, 12), 4, 4));
    assert!(near(pixel(&frame(&p, 14), 4, 4), BLUE), "and ends on the incoming one");
}

#[test]
fn a_dip_to_black_is_black_at_its_middle() {
    let p = transition(TransitionKind::DipToBlack);
    assert!(near(pixel(&frame(&p, 10), 4, 4), RED));
    assert!(near(pixel(&frame(&p, 12), 4, 4), [0, 0, 0]));
    assert!(near(pixel(&frame(&p, 14), 4, 4), BLUE));
}

#[test]
fn a_picture_is_scaled_to_the_size_asked_for() {
    let c = generate(&Pattern::Matte { rgb: BLUE }, 0, 40, 20);
    let s = scale_to(&c, 10, 5);
    assert_eq!((s.width, s.height, s.rgba.len()), (10, 5, 200));
    assert!(near(pixel(&s, 9, 4), BLUE));
}
