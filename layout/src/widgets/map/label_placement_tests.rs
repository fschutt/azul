//! The label placement keeps the map's names still: the same view places the
//! same labels, a small pan never swaps two of them, a higher rank always wins
//! its slot, and a label that comes or goes fades instead of popping.

use super::{
    labels::{LabelCandidate, LabelFrame, LabelPlacement, LabelView, PlacedFrom, LABEL_FADE_SECS},
    TileLabel,
};

const WIDTH: f32 = 800.0;
const HEIGHT: f32 = 600.0;

fn label(text: &str, priority: i32, kind: &str) -> TileLabel {
    TileLabel {
        x: 0.0,
        y: 0.0,
        text: text.to_string(),
        size: 12.0,
        fill: "#333333".to_string(),
        halo: "#ffffff".to_string(),
        priority,
        kind: kind.to_string(),
        italic: false,
        angle: 0.0,
    }
}

/// A view of the world's middle at `zoom`, its centre moved `pan_px` screen
/// pixels to the east.
fn view_at(zoom: f32, pan_px: f64) -> LabelView {
    let world_px = 256.0 * 2f64.powf(f64::from(zoom));
    LabelView {
        width: WIDTH,
        height: HEIGHT,
        margin: 0.0,
        zoom,
        centre: (0.5 + pan_px / world_px, 0.5),
    }
}

/// A label standing at world point `world`, placed where `view` draws it.
fn at<'a>(view: &LabelView, world: (f64, f64), label: &'a TileLabel) -> LabelCandidate<'a> {
    let world_px = view.world_px();
    LabelCandidate {
        x: ((world.0 - view.centre.0) * world_px + f64::from(view.width) * 0.5) as f32,
        y: ((world.1 - view.centre.1) * world_px + f64::from(view.height) * 0.5) as f32,
        label,
        world,
    }
}

/// The world point drawn `dx`, `dy` px from the centre of a view at `zoom`
/// (before any pan).
fn world_near_centre(zoom: f32, dx: f64, dy: f64) -> (f64, f64) {
    let world_px = 256.0 * 2f64.powf(f64::from(zoom));
    (0.5 + dx / world_px, 0.5 + dy / world_px)
}

/// The texts the frame shows at full or rising opacity: the labels that hold
/// a place (a fading-out label is not one of them).
fn shown(frame: &LabelFrame, candidates: &[LabelCandidate<'_>]) -> Vec<String> {
    let mut out: Vec<String> = frame
        .placed
        .iter()
        .filter_map(|p| match p.from {
            PlacedFrom::Candidate(i) => Some(candidates[i].label.text.clone()),
            PlacedFrom::Remembered(_) => None,
        })
        .collect();
    out.sort();
    out
}

/// The opacity the frame gives the label with `text`, if it draws it at all.
fn opacity_of(
    frame: &LabelFrame,
    placement: &LabelPlacement,
    candidates: &[LabelCandidate<'_>],
    text: &str,
) -> Option<f32> {
    frame.placed.iter().find_map(|p| {
        let label = match p.from {
            PlacedFrom::Candidate(i) => candidates[i].label,
            PlacedFrom::Remembered(i) => placement.remembered(i)?,
        };
        (label.text == text).then_some(p.opacity)
    })
}

#[test]
fn the_same_view_placed_twice_shows_the_same_labels_at_the_same_places_with_the_same_ids() {
    let view = view_at(8.0, 0.0);
    let (a, b, c) = (
        label("Vienna", 205, "place"),
        label("Danube", 1050, "waterway"),
        label("Cafe", 1450, "poi"),
    );
    let candidates = vec![
        at(&view, world_near_centre(8.0, 0.0, 0.0), &a),
        at(&view, world_near_centre(8.0, 4.0, 3.0), &c),
        at(&view, world_near_centre(8.0, -120.0, 80.0), &b),
    ];
    let mut placement = LabelPlacement::new();
    let first = placement.place(&candidates, &view, 10.0);
    let second = placement.place(&candidates, &view, 10.5);
    let places = |f: &LabelFrame| -> Vec<(u64, i64, i64)> {
        let mut v: Vec<(u64, i64, i64)> = f
            .placed
            .iter()
            .filter(|p| matches!(p.from, PlacedFrom::Candidate(_)))
            .map(|p| (p.id, p.x.round() as i64, p.y.round() as i64))
            .collect();
        v.sort_unstable();
        v
    };
    assert_eq!(shown(&first, &candidates), shown(&second, &candidates));
    assert_eq!(places(&first), places(&second), "same ids at the same places");
    assert_eq!(
        shown(&first, &candidates),
        vec!["Danube".to_string(), "Vienna".to_string()],
        "the cafe under the city name yields to it"
    );
}

#[test]
fn the_order_of_the_tiles_never_decides_between_equal_labels() {
    let view = view_at(10.0, 0.0);
    let (a, b) = (label("Aachen", 520, "place"), label("Bonn", 520, "place"));
    let (wa, wb) = (
        world_near_centre(10.0, 0.0, 0.0),
        world_near_centre(10.0, 6.0, 2.0),
    );
    let forward = vec![at(&view, wa, &a), at(&view, wb, &b)];
    let backward = vec![at(&view, wb, &b), at(&view, wa, &a)];
    let one = LabelPlacement::new().place(&forward, &view, 0.0);
    let other = LabelPlacement::new().place(&backward, &view, 0.0);
    assert_eq!(shown(&one, &forward).len(), 1, "the two overlap: one shows");
    assert_eq!(
        shown(&one, &forward),
        shown(&other, &backward),
        "which one shows must not depend on which tile was read first"
    );
}

#[test]
fn a_one_pixel_pan_never_swaps_which_of_two_overlapping_labels_shows() {
    let (a, b) = (label("Aachen", 520, "place"), label("Bonn", 520, "place"));
    let (wa, wb) = (
        world_near_centre(10.0, 0.0, 0.0),
        world_near_centre(10.0, 6.0, 2.0),
    );
    let mut placement = LabelPlacement::new();
    let mut seen: Option<Vec<String>> = None;
    for step in 0..24 {
        let view = view_at(10.0, f64::from(step));
        // The tiles come in another order every frame, as the visible range shifts.
        let candidates = if step % 2 == 0 {
            vec![at(&view, wa, &a), at(&view, wb, &b)]
        } else {
            vec![at(&view, wb, &b), at(&view, wa, &a)]
        };
        let frame = placement.place(&candidates, &view, f64::from(step) * 0.016);
        let now = shown(&frame, &candidates);
        assert_eq!(now.len(), 1, "step {step}: exactly one of the two shows");
        match &seen {
            None => seen = Some(now),
            Some(first) => assert_eq!(&now, first, "step {step}: the label swapped"),
        }
    }
}

#[test]
fn a_label_of_strictly_higher_rank_takes_the_slot_and_the_loser_fades_out() {
    let (town, city) = (label("Townsend", 520, "place"), label("Capital", 205, "place"));
    let mut placement = LabelPlacement::new();
    // At zoom 9 the two are 160 px apart; the town holds its place a while.
    let near = view_at(9.0, 0.0);
    let t_world = world_near_centre(9.0, 0.0, 0.0);
    let c_world = world_near_centre(9.0, 160.0, 0.0);
    let candidates = vec![at(&near, t_world, &town), at(&near, c_world, &city)];
    placement.place(&candidates, &near, 0.0);
    let settled = placement.place(&candidates, &near, 1.0);
    assert_eq!(shown(&settled, &candidates).len(), 2);
    // Zoomed out two levels they are 40 px apart and the names overlap: the
    // city has the higher rank and takes the space the town held.
    let far = view_at(7.0, 0.0);
    let candidates = vec![at(&far, t_world, &town), at(&far, c_world, &city)];
    let frame = placement.place(&candidates, &far, 2.0);
    assert_eq!(shown(&frame, &candidates), vec!["Capital".to_string()]);
    assert_eq!(
        opacity_of(&frame, &placement, &candidates, "Capital"),
        Some(1.0),
        "the city stays as it was, it does not fade again"
    );
    let mid = placement.place(&candidates, &far, 2.0 + LABEL_FADE_SECS * 0.5);
    let fading = opacity_of(&mid, &placement, &candidates, "Townsend")
        .expect("the town fades out, it does not vanish");
    assert!(fading > 0.2 && fading < 0.8, "half way out: {fading}");
    let gone = placement.place(&candidates, &far, 2.0 + LABEL_FADE_SECS * 2.0);
    assert_eq!(opacity_of(&gone, &placement, &candidates, "Townsend"), None);
}

#[test]
fn a_new_label_fades_in_instead_of_popping() {
    let view = view_at(6.0, 0.0);
    let city = label("Capital", 205, "place");
    let candidates = vec![at(&view, world_near_centre(6.0, 0.0, 0.0), &city)];
    let mut placement = LabelPlacement::new();
    let first = placement.place(&candidates, &view, 5.0);
    assert_eq!(opacity_of(&first, &placement, &candidates, "Capital"), Some(0.0));
    assert!(first.settled_at > 5.0, "a fade is in flight: the view must be drawn again");
    let half = placement.place(&candidates, &view, 5.0 + LABEL_FADE_SECS * 0.5);
    let o = opacity_of(&half, &placement, &candidates, "Capital").expect("drawn");
    assert!((o - 0.5).abs() < 0.05, "half way in: {o}");
    let done = placement.place(&candidates, &view, 5.0 + LABEL_FADE_SECS);
    assert_eq!(opacity_of(&done, &placement, &candidates, "Capital"), Some(1.0));
    assert!(done.settled_at <= 5.0 + LABEL_FADE_SECS, "nothing left to fade");
}

#[test]
fn a_zoom_across_a_tile_level_keeps_a_shown_label_without_fading_it_again() {
    let city = label("Capital", 205, "place");
    // The same place, as the z5 tile and as the z6 tile quantise it.
    let w5 = world_near_centre(5.9, 0.0, 0.0);
    let w6 = (w5.0 + 1.0e-7, w5.1 - 1.0e-7);
    let mut placement = LabelPlacement::new();
    let v5 = view_at(5.9, 0.0);
    let c5 = vec![at(&v5, w5, &city)];
    placement.place(&c5, &v5, 0.0);
    let before = placement.place(&c5, &v5, 1.0);
    let id = before.placed[0].id;
    let v6 = view_at(6.1, 0.0);
    let c6 = vec![at(&v6, w6, &city)];
    let after = placement.place(&c6, &v6, 1.016);
    assert_eq!(after.placed.len(), 1);
    assert_eq!(after.placed[0].id, id, "the same label, by its identity, not its tile");
    assert_eq!(after.placed[0].opacity, 1.0);
}

#[test]
fn a_river_name_keeps_its_anchor_while_the_view_pans() {
    // Two tiles each anchor the river's name halfway along their piece of it,
    // closer than a name repeats: one of the two shows, and always the same.
    let river = label("Danube", 1050, "waterway");
    let (west, east) = (
        world_near_centre(11.0, -70.0, 10.0),
        world_near_centre(11.0, 80.0, -5.0),
    );
    let mut placement = LabelPlacement::new();
    let mut anchor: Option<i64> = None;
    for step in 0..16 {
        let view = view_at(11.0, f64::from(step));
        let candidates = if step % 2 == 0 {
            vec![at(&view, west, &river), at(&view, east, &river)]
        } else {
            vec![at(&view, east, &river), at(&view, west, &river)]
        };
        let frame = placement.place(&candidates, &view, f64::from(step) * 0.016);
        let held: Vec<i64> = frame
            .placed
            .iter()
            .filter_map(|p| match p.from {
                PlacedFrom::Candidate(i) => Some((candidates[i].world.0 * 1.0e9) as i64),
                PlacedFrom::Remembered(_) => None,
            })
            .collect();
        assert_eq!(held.len(), 1, "step {step}: the name shows once");
        match anchor {
            None => anchor = Some(held[0]),
            Some(a) => assert_eq!(held[0], a, "step {step}: the river's name jumped"),
        }
    }
}

/// A map left alone stops drawing: a crowded view of labels - repeated road
/// names, names of equal rank crossing, the same name offered twice by two
/// tiles - placed again and again with nothing changing settles within a few
/// fades, and its frame says so (`settled_at` is the frame's own time). A
/// placement that never settles keeps the map's fade timer re-rendering the
/// view many times a second while nobody touches it.
#[test]
fn a_crowded_view_left_alone_settles() {
    let kinds = ["place", "transportation_name", "poi", "water_name", "housenumber"];
    let mut labels = Vec::new();
    // A small deterministic generator: the same crowd every run.
    let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for i in 0..220 {
        let text = format!("Name {}", next() % 60);
        let priority = (next() % 6) as i32 * 100;
        labels.push((label(&text, priority, kinds[i % kinds.len()]), next(), next()));
    }
    let view = view_at(14.0, 0.0);
    let mut placed_at: Vec<(f64, f64)> = labels
        .iter()
        .map(|(_, a, b)| {
            world_near_centre(14.0, (a % 900) as f64 - 450.0, (b % 700) as f64 - 350.0)
        })
        .collect();
    // The same name offered by two neighbouring tiles at the same point.
    for i in 0..20 {
        placed_at.push(placed_at[i]);
    }
    let all: Vec<&TileLabel> = labels
        .iter()
        .map(|(l, _, _)| l)
        .chain(labels.iter().take(20).map(|(l, _, _)| l))
        .collect();
    let candidates: Vec<LabelCandidate<'_>> = all
        .iter()
        .zip(&placed_at)
        .map(|(l, w)| at(&view, *w, l))
        .collect();
    let mut placement = LabelPlacement::new();
    let mut last = None;
    for frame_no in 0..240 {
        let now = f64::from(frame_no) * 0.016;
        let frame = placement.place(&candidates, &view, now);
        if frame_no >= 120 {
            assert!(
                frame.settled_at <= now,
                "frame {frame_no}: still fading until {} at {now}",
                frame.settled_at
            );
            let ids: Vec<u64> = frame.placed.iter().map(|p| p.id).collect();
            if let Some(previous) = &last {
                assert_eq!(&ids, previous, "frame {frame_no}: the labels changed");
            }
            last = Some(ids);
        }
    }
}

/// At a low zoom the world is narrower than the view and shows twice: the
/// same label stands at the same world point in both copies, one world width
/// apart on the screen. Left alone, that view settles too.
#[test]
fn a_world_shown_twice_left_alone_settles() {
    let zoom = 2.0;
    let view = LabelView { width: 1100.0, height: 720.0, margin: 0.0, zoom, centre: (0.5, 0.5) };
    let world_px = view.world_px() as f32;
    let names: Vec<TileLabel> = (0..40)
        .map(|i| label(&format!("Country {i}"), (i % 4) * 100, "place"))
        .collect();
    let mut candidates = Vec::new();
    for (i, l) in names.iter().enumerate() {
        let world = (0.02 + 0.024 * i as f64, 0.3 + 0.01 * (i % 7) as f64);
        let one = at(&view, world, l);
        candidates.push(one);
        candidates.push(LabelCandidate { x: one.x + world_px, ..one });
        candidates.push(LabelCandidate { x: one.x - world_px, ..one });
    }
    let mut placement = LabelPlacement::new();
    for frame_no in 0..240 {
        let now = f64::from(frame_no) * 0.016;
        let frame = placement.place(&candidates, &view, now);
        if frame_no >= 120 {
            assert!(
                frame.settled_at <= now,
                "frame {frame_no}: still fading until {} at {now}",
                frame.settled_at
            );
        }
    }
}

/// Two different labels of one name and one layer can stand a few pixels
/// apart: the tiles carry "Australia" once as the country and once as the
/// continent, both in `place`. They share an identity, so either may be taken
/// for the one shown last frame. Left alone, the view still settles: the
/// country keeps its slot and its node, and the continent stays held back.
#[test]
fn two_labels_of_one_name_standing_together_settle_instead_of_taking_turns() {
    let view = view_at(2.0, 0.0);
    let country = TileLabel {
        size: 13.0,
        ..label("Australia", 100, "place")
    };
    let continent = TileLabel {
        size: 14.0,
        ..label("Australia", 300, "place")
    };
    // The continent's anchor is a pixel north of the country's, so it comes
    // first wherever the placement walks the labels by their place.
    let candidates = vec![
        at(&view, world_near_centre(2.0, 0.0, 0.0), &country),
        at(&view, world_near_centre(2.0, -3.0, -1.0), &continent),
    ];
    let mut placement = LabelPlacement::new();
    let mut held: Option<u64> = None;
    for frame_no in 0..120 {
        let now = 10.0 + f64::from(frame_no) * 0.016;
        let frame = placement.place(&candidates, &view, now);
        assert_eq!(
            shown(&frame, &candidates),
            vec!["Australia".to_string()],
            "frame {frame_no}: one of the two shows"
        );
        if now < 10.0 + LABEL_FADE_SECS * 2.0 {
            continue;
        }
        assert!(
            frame.settled_at <= now,
            "frame {frame_no}: still fading until {} at {now} - the two take turns",
            frame.settled_at
        );
        let id = frame
            .placed
            .iter()
            .find(|p| p.from == PlacedFrom::Candidate(0))
            .map(|p| p.id)
            .expect("the country, which outranks the continent, is the one shown");
        match held {
            None => held = Some(id),
            Some(first) => assert_eq!(id, first, "frame {frame_no}: the country's node changed"),
        }
    }
}
