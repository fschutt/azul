//! The frame's rules that need no window: where the sidebar starts, what the frame keeps clear
//! of, and where the status band's lanes stand once the frame is padded.

use azul::widgets::SidebarState;

use super::*;

fn lane(x: f32, width: f32) -> LogicalRect {
    LogicalRect {
        origin: LogicalPosition { x, y: 0.0 },
        size: azul::css::LogicalSize { width, height: 48.0 },
    }
}

fn band() -> StatusBarBand {
    StatusBarBand {
        lanes: vec![lane(10.0, 100.0), lane(200.0, 200.0)].into(),
        height: 48.0,
        width: 411.0,
    }
}

#[test]
fn a_phone_starts_with_the_drawer_shut_and_a_desktop_with_the_sidebar_open() {
    assert_eq!(initial_sidebar(true), SidebarState::Rail);
    assert_eq!(initial_sidebar(false), SidebarState::Expanded);
}

#[test]
fn a_window_the_engine_insets_keeps_clear_of_nothing_but_the_keyboard() {
    let none = Insets::new(0.0, 0.0, 0.0, 0.0, 0.0);
    assert_eq!(pads(&none, false), (0.0, 0.0, 0.0));
    // the engine already insets the root by the bars: only what the keyboard covers beyond the bar is left
    let bars = Insets::new(24.0, 0.0, 24.0, 0.0, 0.0);
    assert_eq!(pads(&bars, false), (0.0, 0.0, 0.0), "the four sides are the engine's");
    let keyboard = Insets::new(24.0, 0.0, 24.0, 0.0, 324.0);
    assert_eq!(pads(&keyboard, false), (0.0, 0.0, 300.0), "less the bar the root already excludes");
}

#[test]
fn a_phone_that_draws_under_its_bars_keeps_clear_of_the_gesture_bar_and_rises_by_the_keyboard() {
    // a Pixel 8 held upright: the status bar is the tab strip's (top: not padded), the gesture bar at the bottom
    let portrait = Insets::new(48.0, 0.0, 24.0, 0.0, 0.0);
    assert_eq!(pads(&portrait, true), (0.0, 0.0, 24.0));
    // the keyboard stands on the gesture bar's place: nothing of the bar, all of the keyboard
    let typing = Insets::new(48.0, 0.0, 24.0, 0.0, 324.0);
    assert_eq!(pads(&typing, true), (0.0, 0.0, 324.0));
    // an iPhone on its side: the notch at the left, the home indicator at the bottom
    let landscape = Insets::new(0.0, 0.0, 21.0, 47.0, 0.0);
    assert_eq!(pads(&landscape, true), (47.0, 0.0, 21.0));
}

#[test]
fn a_band_is_left_as_it_is_where_nothing_pads_the_frame_at_the_sides() {
    assert_eq!(band_in_box(band(), 0.0, 0.0), band());
    assert_eq!(band_in_box(band(), -3.0, f32::NAN.min(0.0)), band(), "an inset that is none is none");
}

#[test]
fn a_band_moves_its_lanes_by_the_padding_so_the_strip_is_not_moved_twice() {
    let shifted = band_in_box(band(), 10.0, 5.0);
    let xs: Vec<f32> = shifted.lanes.as_slice().iter().map(|l| l.origin.x).collect();
    assert_eq!(xs, vec![0.0, 190.0]);
    assert_eq!(shifted.height, 48.0, "the band's height is the window's");
    assert_eq!(shifted.width, 396.0, "the box is narrower by both paddings");
    let widths: Vec<f32> = shifted.lanes.as_slice().iter().map(|l| l.size.width).collect();
    assert_eq!(widths, vec![100.0, 200.0], "a lane keeps its width");
}

#[test]
fn a_band_wider_than_a_box_never_has_a_negative_width() {
    let tiny = StatusBarBand { lanes: vec![lane(0.0, 5.0)].into(), height: 48.0, width: 20.0 };
    assert_eq!(band_in_box(tiny, 30.0, 30.0).width, 0.0);
}
