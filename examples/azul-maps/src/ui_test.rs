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
    // a desktop window narrower than the panel's width: its controls (macOS's lights at the left) keep their strip over the bar
    let narrow_desktop = Frame { title: kit::TitleInsets { left: 78.0, right: 0.0, height: 28.0 }, ..frame(500.0, 700.0, 0.0, 0.0) };
    assert!(!narrow_desktop.wide());
    assert_eq!(narrow_desktop.top(), 44.0, "under the window's controls, as in the wide layout");
}

#[test]
fn the_notice_is_a_short_bar_beside_the_panel_and_a_full_bar_above_a_phones_sheet() {
    // a desktop: 360 px wide, centred on the map between the panel and the keys (the panel and its gap cover 362 px, the keys 84)
    let room = 1280.0 - PANEL_COVER - KEYS_COVER;
    assert_eq!(toast_place(true, PANEL_COVER, 1280.0, 0.0), format!("left: {:.0}px; width: 360px; bottom: 70px;", PANEL_COVER + (room - 360.0) / 2.0));
    // the panel folded away: centred on the map that is free (the keys at the right keep their strip)
    assert_eq!(toast_place(true, 0.0, 1280.0, 0.0), format!("left: {:.0}px; width: 360px; bottom: 70px;", (1280.0 - KEYS_COVER - 360.0) / 2.0));
    // a window with little room beside the panel: the notice is no wider than the room
    assert_eq!(
        toast_place(true, PANEL_COVER, 560.0, 0.0),
        format!("left: {:.0}px; width: {:.0}px; bottom: 70px;", PANEL_COVER, 560.0 - PANEL_COVER - KEYS_COVER)
    );
    // a phone: between the map's sides, 70 px above the sheet
    assert_eq!(toast_place(false, 0.0, 411.0, 168.0), "left: 12px; right: 84px; bottom: 238px;");
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
