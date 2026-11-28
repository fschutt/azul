//! Where AzMaps puts what floats on the map: the top, the sheet's heights, the form of the window.

use super::*;

fn frame(width: f32, height: f32, top_inset: f32, keyboard: f32) -> Frame {
    Frame {
        size: (width, height),
        insets: Insets::new(top_inset, 0.0, 0.0, 0.0, keyboard),
        title: kit::TitleInsets { left: 0.0, right: 0.0, height: 28.0 },
    }
}

#[test]
fn a_window_from_560_px_is_wide_and_a_phone_upright_is_not() {
    assert!(frame(1280.0, 800.0, 0.0, 0.0).wide());
    assert!(frame(914.0, 411.0, 0.0, 0.0).wide(), "a phone on its side has the panel");
    assert!(!frame(411.0, 914.0, 48.0, 0.0).wide());
}

#[test]
fn what_floats_stands_under_the_window_controls_on_a_desktop_and_the_status_bar_on_a_phone() {
    assert_eq!(frame(1280.0, 800.0, 0.0, 0.0).top(), 44.0, "40 px for the controls and a gap");
    let tall_controls = Frame { title: kit::TitleInsets { left: 78.0, right: 0.0, height: 46.0 }, ..frame(1280.0, 800.0, 0.0, 0.0) };
    assert_eq!(tall_controls.top(), 50.0, "a title bar taller than 40 px lifts it");
    assert_eq!(frame(411.0, 914.0, 48.0, 0.0).top(), 48.0, "under the status bar and the cutout");
    assert_eq!(frame(411.0, 914.0, 0.0, 0.0).top(), 8.0, "a phone with no inset keeps a gap");
}

#[test]
fn the_sheets_heights_are_the_peek_half_the_screen_and_the_screen_less_the_bar() {
    let f = frame(411.0, 914.0, 48.0, 0.0);
    assert_eq!(sheet_height(SheetDetent::Peek, &f), PEEK_HEIGHT);
    assert_eq!(sheet_height(SheetDetent::Half, &f), 457.0);
    let full = sheet_height(SheetDetent::Full, &f);
    assert_eq!(full, 914.0 - (48.0 + TOP_ROOM), "the search bar stays in sight above the full sheet");
}

#[test]
fn a_short_screen_keeps_the_sheets_heights_in_order() {
    let f = frame(360.0, 400.0, 24.0, 0.0);
    let (peek, half, full) = (
        sheet_height(SheetDetent::Peek, &f),
        sheet_height(SheetDetent::Half, &f),
        sheet_height(SheetDetent::Full, &f),
    );
    assert!(peek <= half && half <= full && full <= 400.0, "{peek} {half} {full}");
}
