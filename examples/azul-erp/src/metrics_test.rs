//! The room the table has, from what the window is made of.

use super::*;

fn desktop() -> Room {
    Room {
        root: (1280.0, 800.0),
        phone: false,
        sidebar: 240.0,
        preview: 320.0,
        footer: true,
    }
}

#[test]
fn a_desktop_table_has_the_window_less_the_sidebar_the_preview_the_ribbon_and_the_status_line() {
    let (w, h) = body(&desktop());
    assert_eq!(w, 1280.0 - 240.0 - 320.0);
    assert_eq!(h, 800.0 - RIBBON_DESKTOP - FOOTER);
}

#[test]
fn a_phone_table_has_the_rail_beside_it_and_the_preview_under_it() {
    let room = Room {
        root: (411.0, 915.0),
        phone: true,
        sidebar: 0.0,
        preview: 130.0,
        footer: true,
    };
    let (w, h) = body(&room);
    assert_eq!(w, 411.0 - RAIL);
    assert_eq!(h, 915.0 - RIBBON_PHONE - FOOTER - 130.0);
}

#[test]
fn without_a_preview_or_a_status_line_the_table_has_the_room_they_would_have_taken() {
    let mut room = desktop();
    room.preview = 0.0;
    room.footer = false;
    let (w, h) = body(&room);
    assert_eq!(w, 1280.0 - 240.0);
    assert_eq!(h, 800.0 - RIBBON_DESKTOP);
}

#[test]
fn a_window_that_is_no_size_still_lays_a_table_out() {
    for bad in [0.0, -5.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let room = Room {
            root: (bad, bad),
            phone: false,
            sidebar: bad,
            preview: bad,
            footer: true,
        };
        assert_eq!(body(&room), (MIN_WIDTH, MIN_HEIGHT), "{bad}");
    }
}

#[test]
fn a_size_beyond_the_cap_is_held_to_it() {
    assert_eq!(clean(1.0e9), MAX_PX);
    assert_eq!(clean(f32::NAN), 0.0);
    assert_eq!(clean(-1.0), 0.0);
    assert_eq!(clean(12.5), 12.5);
    let room = Room {
        root: (1.0e12, 1.0e12),
        phone: false,
        sidebar: 0.0,
        preview: 0.0,
        footer: false,
    };
    let (w, h) = body(&room);
    assert!(w <= MAX_PX && h <= MAX_PX, "{w} x {h}");
}

#[test]
fn a_small_phone_keeps_a_table_a_finger_can_use() {
    // 360 x 640 with the peek of the Preview under the table
    let room = Room {
        root: (360.0, 640.0),
        phone: true,
        sidebar: 0.0,
        preview: 130.0,
        footer: true,
    };
    let (w, h) = body(&room);
    assert_eq!(w, 342.0);
    assert!(h >= MIN_HEIGHT, "{h}");
}

#[test]
fn the_table_is_the_body_less_a_header_and_the_hairlines() {
    let room = desktop();
    let (bw, bh) = body(&room);
    let (w, h) = table(&room, 0.0);
    assert_eq!((w, h), (bw - 4.0, bh - 2.0));
    let (_, with_header) = table(&room, 48.0);
    assert_eq!(with_header, bh - 48.0 - 2.0);
}
