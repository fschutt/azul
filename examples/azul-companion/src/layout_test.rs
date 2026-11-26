//! The zones as numbers: they fill the safe area, never overlap, and every touch target is big enough, for every size
//! of the study in both orientations and with or without the safe area.

use super::*;
use crate::env::{Lang, DEVICES};

/// Every environment of the study: three phones, both ways round, with and without insets.
fn envs() -> Vec<Env> {
    let mut all = Vec::new();
    for device in &DEVICES {
        for landscape in [false, true] {
            for safe in [true, false] {
                all.push(device.env(landscape, Lang::En, safe));
            }
        }
    }
    all
}

fn named(z: &Zones) -> Vec<(&'static str, Zone)> {
    if z.side.w > 0.0 {
        vec![("header", z.header), ("pad", z.pad), ("controls", z.controls), ("side", z.side)]
    } else {
        vec![
            ("header", z.header),
            ("filebar", z.filebar),
            ("pad", z.pad),
            ("controls", z.controls),
            ("context", z.context),
            ("dock", z.dock),
            ("tabs", z.tabs),
        ]
    }
}

#[test]
fn the_zones_lie_inside_the_safe_area() {
    for env in envs() {
        let z = zones(&env);
        let safe = Zone { x: env.insets.left, y: env.insets.top, w: env.usable().0, h: env.usable().1 };
        assert_eq!(z.safe, safe, "{env:?}");
        for (name, zone) in named(&z) {
            assert!(
                zone.x >= safe.x - 0.01 && zone.y >= safe.y - 0.01 && zone.right() <= safe.right() + 0.01 && zone.bottom() <= safe.bottom() + 0.01,
                "{name} {zone:?} leaves {safe:?} in {env:?}"
            );
        }
    }
}

#[test]
fn no_two_zones_overlap() {
    for env in envs() {
        let z = zones(&env);
        let all = named(&z);
        for (i, (a, za)) in all.iter().enumerate() {
            for (b, zb) in &all[i + 1..] {
                assert!(!za.overlaps(zb), "{a} {za:?} and {b} {zb:?} overlap in {env:?}");
            }
        }
    }
}

#[test]
fn in_portrait_the_zones_stack_from_the_top_to_the_bottom_with_no_gap() {
    for env in envs().into_iter().filter(|e| !e.landscape()) {
        let z = zones(&env);
        let stack = [z.header, z.filebar, z.pad, z.controls, z.context, z.dock, z.tabs];
        let mut y = z.safe.y;
        for zone in stack {
            assert!((zone.y - y).abs() < 0.01, "{zone:?} starts at {} not {y} in {env:?}", zone.y);
            y = zone.bottom();
        }
        assert!((y - z.safe.bottom()).abs() < 0.01, "the stack ends at {y}, the safe area at {} in {env:?}", z.safe.bottom());
    }
}

#[test]
fn in_landscape_the_pad_column_and_the_side_panel_fill_the_width_and_the_height() {
    for env in envs().into_iter().filter(|e| e.landscape()) {
        let z = zones(&env);
        assert!(z.side.w > 0.0, "{env:?}");
        assert!((z.header.y - z.safe.y).abs() < 0.01 && (z.header.bottom() - z.pad.y).abs() < 0.01, "{env:?}");
        assert!((z.pad.bottom() - z.controls.y).abs() < 0.01 && (z.controls.bottom() - z.safe.bottom()).abs() < 0.01, "{env:?}");
        assert!((z.header.x - z.safe.x).abs() < 0.01 && (z.side.right() - z.safe.right()).abs() < 0.01, "{env:?}");
        assert!(z.header.right() <= z.side.x + 0.01, "{env:?}");
        assert!((z.side.y - z.safe.y).abs() < 0.01 && (z.side.bottom() - z.safe.bottom()).abs() < 0.01, "{env:?}");
        assert!((z.tabs.bottom() - z.side_body.y).abs() < 0.01, "the switch is above the body: {env:?}");
        assert!((z.side_body.bottom() - z.safe.bottom()).abs() < 0.01, "{env:?}");
    }
}

#[test]
fn the_pad_is_big_enough_to_use_in_every_size() {
    for env in envs() {
        let z = zones(&env);
        assert!(z.pad.w >= 200.0 && z.pad.h >= 180.0, "the pad is {:?} in {env:?}", z.pad);
    }
}

#[test]
fn every_control_row_is_a_touch_target_tall() {
    for env in envs() {
        let z = zones(&env);
        assert!(z.controls.h >= MIN_TARGET, "{env:?}");
        assert!(z.tabs.h >= MIN_TARGET, "{env:?}");
        assert!(z.header.h >= MIN_TARGET - 4.0, "the header is not a target but must not be tiny: {env:?}");
        assert!(z.button_h >= 48.0, "a quick button is at least 48 tall: {env:?}");
        assert!(z.button_gap >= 8.0, "{env:?}");
    }
}

#[test]
fn the_four_quick_buttons_fit_their_dock() {
    for env in envs() {
        let z = zones(&env);
        let needed = if env.landscape() {
            4.0 * z.button_h + 3.0 * z.button_gap
        } else {
            2.0 * z.button_h + z.button_gap
        };
        assert!(z.dock.h >= needed - 0.01, "the dock is {} tall and the buttons need {needed} in {env:?}", z.dock.h);
        if env.landscape() {
            assert!(z.dock.bottom() <= z.side_body.bottom() + 0.01 && z.dock.y >= z.side_body.y - 0.01, "{env:?}");
        }
    }
}

#[test]
fn a_zone_that_has_no_room_is_empty_and_never_negative() {
    let tiny = Env { width: 100.0, height: 100.0, insets: crate::env::Insets::NONE, platform: crate::env::Platform::Ios, lang: Lang::En };
    let z = zones(&tiny);
    for (name, zone) in named(&z) {
        assert!(zone.w >= 0.0 && zone.h >= 0.0, "{name} {zone:?}");
    }
}

// ---- the page inside the pad

#[test]
fn the_page_keeps_its_shape_and_sits_inside_the_pad_left_of_the_strip() {
    let g = pad_geometry(360.0, 390.0, Some(0.707));
    assert!((g.page.w / g.page.h - 0.707).abs() < 0.01, "{g:?}");
    assert!(g.page.x >= PAD_MARGIN - 0.01 && g.page.y >= PAD_MARGIN - 0.01);
    assert!(g.page.right() <= 360.0 - STRIP_W + 0.01 && g.page.bottom() <= 390.0 - PAD_MARGIN + 0.01, "{g:?}");
    assert!(!g.page.overlaps(&g.strip), "{g:?}");
    assert!((g.strip.right() - 360.0).abs() < 0.01 && (g.strip.h - 390.0).abs() < 0.01 && (g.strip.w - STRIP_W).abs() < 0.01);
}

#[test]
fn a_page_wider_than_the_pad_is_fitted_by_its_width_and_a_taller_one_by_its_height() {
    let wide = pad_geometry(400.0, 300.0, Some(2.0));
    assert!((wide.page.w - (400.0 - 2.0 * PAD_MARGIN - STRIP_W)).abs() < 0.01, "{wide:?}");
    let tall = pad_geometry(400.0, 300.0, Some(0.25));
    assert!((tall.page.h - (300.0 - 2.0 * PAD_MARGIN)).abs() < 0.01, "{tall:?}");
}

#[test]
fn an_unknown_page_is_an_a4_sheet() {
    let a = pad_geometry(360.0, 390.0, None);
    let b = pad_geometry(360.0, 390.0, Some(DEFAULT_ASPECT));
    assert_eq!(a, b);
    let bad = pad_geometry(360.0, 390.0, Some(f32::NAN));
    assert_eq!(bad, b, "an aspect that is no number is an A4 sheet too");
    assert_eq!(pad_geometry(360.0, 390.0, Some(0.0)), b);
    assert_eq!(pad_geometry(360.0, 390.0, Some(-1.0)), b);
}

#[test]
fn a_pad_with_no_room_has_a_page_of_no_size_and_never_panics() {
    for (w, h) in [(0.0, 0.0), (10.0, 10.0), (60.0, 400.0), (f32::NAN, 100.0), (-5.0, 100.0)] {
        let g = pad_geometry(w, h, Some(0.7));
        assert!(g.page.w >= 0.0 && g.page.h >= 0.0 && g.page.w.is_finite() && g.page.h.is_finite(), "{w}x{h}: {g:?}");
    }
}

#[test]
fn the_page_is_centred_in_the_room_the_strip_leaves() {
    let g = pad_geometry(400.0, 500.0, Some(1.0));
    let room_x = PAD_MARGIN + (400.0 - STRIP_W - 2.0 * PAD_MARGIN) / 2.0;
    assert!((g.page.x + g.page.w / 2.0 - room_x).abs() < 0.01, "{g:?}");
    assert!((g.page.y + g.page.h / 2.0 - 250.0).abs() < 0.01, "{g:?}");
}
