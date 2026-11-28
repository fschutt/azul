use super::*;

#[test]
fn a_tap_on_a_side_turns_the_page_and_one_in_the_middle_shows_the_toolbar() {
    assert_eq!(tap_at(10.0, 400.0), Tap::Previous);
    assert_eq!(tap_at(119.0, 400.0), Tap::Previous);
    assert_eq!(tap_at(200.0, 400.0), Tap::Toggle);
    assert_eq!(tap_at(281.0, 400.0), Tap::Next);
    assert_eq!(tap_at(399.0, 400.0), Tap::Next);
}

#[test]
fn a_tap_that_cannot_be_placed_is_the_middles() {
    assert_eq!(tap_at(5.0, 0.0), Tap::Toggle);
    assert_eq!(tap_at(f32::NAN, 400.0), Tap::Toggle);
    assert_eq!(tap_at(-3.0, 400.0), Tap::Toggle);
    assert_eq!(tap_at(401.0, 400.0), Tap::Toggle);
}

#[test]
fn the_zoom_reads_in_steps_of_five_percent_around_the_default() {
    assert_eq!(zoom_percent(BASE_FONT_PX), 100);
    assert_eq!(zoom_percent(20), 110);
    assert_eq!(zoom_percent(12), 65);
    let (low, high) = zoom_range();
    assert!(low < 100 && 100 < high, "{low} {high}");
    assert_eq!(high, zoom_percent(32));
}

#[test]
fn the_fit_button_goes_round_the_three_layouts() {
    let mut layout = PageLayout::Auto;
    let mut seen = Vec::new();
    for _ in 0..3 {
        seen.push(layout);
        layout = next_layout(layout);
    }
    assert_eq!(layout, PageLayout::Auto);
    assert_eq!(seen, [PageLayout::Auto, PageLayout::Single, PageLayout::Spread]);
    let icons: std::collections::BTreeSet<&str> = seen.iter().map(|l| layout_icon(*l)).collect();
    assert_eq!(icons.len(), 3, "each layout has its own icon");
}

#[test]
fn the_page_field_shows_a_page_from_one_and_reads_one_typed_back() {
    assert_eq!(page_number(0, 12), 1);
    assert_eq!(page_number(11, 12), 12);
    assert_eq!(page_number(40, 12), 12, "past the end shows the last page");
    assert_eq!(page_number(0, 0), 1, "a chapter not laid out is page 1");
    assert_eq!(typed_page("3", 12), Some(2));
    assert_eq!(typed_page(" 12 ", 12), Some(11));
    assert_eq!(typed_page("99", 12), Some(11), "past the end is the last page");
    assert_eq!(typed_page("0", 12), Some(0));
    assert_eq!(typed_page("-4", 12), Some(0));
    assert_eq!(typed_page("abc", 12), None);
    assert_eq!(typed_page("", 12), None);
    assert_eq!(typed_page("5", 0), Some(0));
}

#[test]
fn the_slider_is_thousandths_of_the_books_progress() {
    assert_eq!(slider_value(0.0), 0.0);
    assert_eq!(slider_value(1.0), SLIDER_MAX);
    assert_eq!(slider_value(0.4126), 413.0);
    assert_eq!(slider_value(f32::NAN), 0.0);
    assert_eq!(slider_value(7.0), SLIDER_MAX);
    assert!((slider_progress(slider_value(0.25)) - 0.25).abs() < 1e-6);
    assert_eq!(slider_progress(-5.0), 0.0);
    assert_eq!(slider_progress(5_000.0), 1.0);
    assert_eq!(slider_progress(f32::NAN), 0.0);
}

#[test]
fn the_pages_have_the_window_less_what_floats_not() {
    // A desktop: the toolbar's row and, while it is open, the strip's column come off.
    assert_eq!(reading_room((1000.0, 700.0), false, false), (1000.0, 700.0 - TOOLBAR_PX));
    assert_eq!(reading_room((1000.0, 700.0), false, true), (1000.0 - STRIP_PX, 700.0 - TOOLBAR_PX));
    // A phone: the page fills the screen, whatever shows over it.
    assert_eq!(reading_room((412.0, 915.0), true, true), (412.0, 915.0));
    // A window nothing fits in keeps a page of some size.
    assert_eq!(reading_room((50.0, 30.0), false, true), (120.0, 120.0));
}

#[test]
fn the_pages_frame_takes_the_arrows_and_gutters_of_a_desktop_but_not_of_a_phone() {
    let (desktop_across, desktop_down) = page_allowance(false);
    let (phone_across, phone_down) = page_allowance(true);
    assert_eq!(desktop_across, 2.0 * ARROW_PX + 2.0 * GUTTER_PX + 4.0);
    assert_eq!(phone_across, 4.0, "a phone's page has only its borders");
    assert_eq!(desktop_down, LINES_PX);
    assert_eq!(phone_down, LINES_PX);
    assert!(phone_across < desktop_across);
}
