//! The numbers of the sidebar family: widths in each state, the phone's pushing drawer, the window
//! controls' room, the Inspector's limits and where its pane settles.

use super::*;

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.001
}

// ---- the toggle ----

#[test]
fn the_toggle_swaps_expanded_and_rail_and_never_shows_the_handle_by_default() {
    assert_eq!(SidebarState::Expanded.toggled(false), SidebarState::Rail);
    assert_eq!(SidebarState::Rail.toggled(false), SidebarState::Expanded);
    // a sidebar that was set to the handle in code is brought back to the rail, not past it
    assert_eq!(SidebarState::Handle.toggled(false), SidebarState::Rail);
}

#[test]
fn with_the_handle_in_the_round_the_toggle_goes_expanded_rail_handle_expanded() {
    let mut s = SidebarState::Expanded;
    let mut seen = Vec::new();
    for _ in 0..4 {
        s = s.toggled(true);
        seen.push(s);
    }
    assert_eq!(
        seen,
        vec![
            SidebarState::Rail,
            SidebarState::Handle,
            SidebarState::Expanded,
            SidebarState::Rail
        ]
    );
}

#[test]
fn a_click_on_the_handle_opens_the_rail_and_nothing_else_moves() {
    assert_eq!(SidebarState::Handle.from_handle(), SidebarState::Rail);
    assert_eq!(SidebarState::Rail.from_handle(), SidebarState::Rail);
    assert_eq!(SidebarState::Expanded.from_handle(), SidebarState::Expanded);
}

#[test]
fn the_default_state_is_the_rail() {
    assert_eq!(SidebarState::default(), SidebarState::Rail);
}

// ---- the left column ----

#[test]
fn the_rail_is_54_on_a_desktop_and_48_on_a_phone() {
    assert!(near(rail_width(FormFactor::Desktop, 0.0), 54.0));
    assert!(near(rail_width(FormFactor::Phone, 0.0), 48.0));
}

#[test]
fn the_rail_is_never_narrower_than_the_window_controls_beside_it() {
    // the macOS traffic lights are 78 px of room: the rail keeps them
    assert!(near(rail_width(FormFactor::Desktop, 78.0), 78.0));
    // a window whose controls are at the right has none at the left
    assert!(near(rail_width(FormFactor::Desktop, 0.0), 54.0));
    // a phone has no window controls whatever it is told
    assert!(near(rail_width(FormFactor::Phone, 78.0), 48.0));
    assert!(near(rail_width(FormFactor::Desktop, f32::NAN), 54.0));
    assert!(near(rail_width(FormFactor::Desktop, -30.0), 54.0));
}

#[test]
fn the_handle_is_16_wide_in_both() {
    for form in [FormFactor::Desktop, FormFactor::Phone] {
        assert!(near(column_width(SidebarState::Handle, form, 0.0, 78.0).px, 16.0));
    }
}

#[test]
fn an_expanded_desktop_sidebar_is_its_width_held_to_its_limits() {
    let w = |wanted: f32| column_width(SidebarState::Expanded, FormFactor::Desktop, wanted, 0.0).px;
    assert!(near(w(0.0), EXPANDED_WIDTH_DEFAULT), "no width: the default");
    assert!(near(w(f32::NAN), EXPANDED_WIDTH_DEFAULT));
    assert!(near(w(f32::INFINITY), EXPANDED_WIDTH_DEFAULT));
    assert!(near(w(100.0), EXPANDED_WIDTH_MIN));
    assert!(near(w(300.0), 300.0));
    assert!(near(w(5000.0), EXPANDED_WIDTH_MAX));
}

#[test]
fn an_expanded_desktop_sidebar_is_never_narrower_than_the_window_controls() {
    let wide = column_width(SidebarState::Expanded, FormFactor::Desktop, 160.0, 0.0).px;
    assert!(near(wide, 160.0));
    // controls wider than the minimum would stick out of it
    let kept = column_width(SidebarState::Expanded, FormFactor::Desktop, 160.0, 200.0).px;
    assert!(near(kept, 200.0));
}

#[test]
fn a_phone_s_drawer_is_76_percent_of_the_frame_up_to_360() {
    let col = column_width(SidebarState::Expanded, FormFactor::Phone, 0.0, 0.0);
    assert!(near(col.percent, 76.0));
    assert!(near(col.max_px, 360.0));
    assert!(near(col.in_frame(411.0), 411.0 * 0.76));
    assert!(near(col.in_frame(1000.0), 360.0), "a wide frame gets the cap");
    assert!(near(col.in_frame(-5.0), 0.0));
}

#[test]
fn a_fixed_column_ignores_the_frame() {
    let col = column_width(SidebarState::Rail, FormFactor::Desktop, 0.0, 0.0);
    assert!(near(col.in_frame(10.0), 54.0));
    assert!(near(col.in_frame(1000.0), 54.0));
}

#[test]
fn only_a_phone_s_open_drawer_pushes_the_main_view() {
    for (state, form, pushes) in [
        (SidebarState::Expanded, FormFactor::Phone, true),
        (SidebarState::Rail, FormFactor::Phone, false),
        (SidebarState::Handle, FormFactor::Phone, false),
        (SidebarState::Expanded, FormFactor::Desktop, false),
        (SidebarState::Rail, FormFactor::Desktop, false),
    ] {
        assert_eq!(pushes_main(state, form), pushes, "{state:?} on {form:?}");
    }
}

#[test]
fn the_window_controls_band_is_the_title_bar_s_height_where_the_controls_are_at_the_left() {
    assert!(near(controls_height(78.0, 28.0), 28.0));
    assert!(near(controls_height(0.0, 28.0), 0.0), "controls at the right: no band at the left");
    assert!(near(controls_height(78.0, f32::NAN), 0.0));
    assert!(near(controls_height(78.0, -3.0), 0.0));
}

#[test]
fn the_tab_row_keeps_the_controls_the_sidebar_does_not_cover() {
    // no sidebar: the whole of the controls' room
    assert!(near(header_left_pad(78.0, 0.0), 78.0));
    // the 16 px handle covers a little
    assert!(near(header_left_pad(78.0, 16.0), 62.0));
    // a rail as wide as the controls, an expanded sidebar wider: nothing left to clear
    assert!(near(header_left_pad(78.0, 78.0), 0.0));
    assert!(near(header_left_pad(78.0, 240.0), 0.0));
    assert!(near(header_left_pad(0.0, 54.0), 0.0));
    assert!(near(header_left_pad(f32::NAN, f32::NAN), 0.0));
}

// ---- the Inspector ----

#[test]
fn the_inspector_docks_right_on_a_desktop_and_at_the_bottom_on_a_phone() {
    assert_eq!(InspectorDock::Auto.resolved(FormFactor::Desktop), InspectorDock::Right);
    assert_eq!(InspectorDock::Auto.resolved(FormFactor::Phone), InspectorDock::Bottom);
    // a dock the app names stays
    assert_eq!(InspectorDock::Right.resolved(FormFactor::Phone), InspectorDock::Right);
    assert_eq!(InspectorDock::Bottom.resolved(FormFactor::Desktop), InspectorDock::Bottom);
}

#[test]
fn the_right_dock_s_width_stays_between_200_and_60_percent_and_leaves_the_body_240() {
    // a 1200 px container: 60 % is 720, the body keeps 240, so the limit is 720
    assert!(near(clamp_inspector_width(320.0, 1200.0), 320.0));
    assert!(near(clamp_inspector_width(100.0, 1200.0), 200.0));
    assert!(near(clamp_inspector_width(2000.0, 1200.0), 720.0));
    // a 600 px container: 60 % is 360, but the body keeps 240 so 360 is the limit (600 - 240)
    assert!(near(clamp_inspector_width(2000.0, 600.0), 360.0));
    // a 400 px container is too narrow for both limits: the minimum wins
    assert!(near(clamp_inspector_width(300.0, 400.0), 200.0));
}

#[test]
fn a_dock_width_that_is_no_number_is_the_default_held_to_the_limits() {
    assert!(near(clamp_inspector_width(f32::NAN, 1200.0), INSPECTOR_WIDTH_DEFAULT));
    assert!(near(clamp_inspector_width(-5.0, 1200.0), INSPECTOR_WIDTH_DEFAULT));
    assert!(near(clamp_inspector_width(f32::INFINITY, 1200.0), INSPECTOR_WIDTH_DEFAULT));
    // an unknown container: the width is only held to the minimum
    assert!(near(clamp_inspector_width(320.0, f32::NAN), 320.0));
    assert!(near(clamp_inspector_width(120.0, 0.0), 200.0));
}

#[test]
fn the_bottom_pane_stays_between_the_peek_and_85_percent_of_its_container() {
    assert!(near(pane_max(800.0), 680.0));
    assert!(near(clamp_pane_height(168.0, 800.0), 168.0));
    assert!(near(clamp_pane_height(10.0, 800.0), PANE_PEEK));
    assert!(near(clamp_pane_height(5000.0, 800.0), 680.0));
    assert!(near(clamp_pane_height(f32::NAN, 800.0), PANE_HEIGHT_DEFAULT));
    // a container too short for the peek still gets the peek
    assert!(near(pane_max(10.0), PANE_PEEK));
    assert!(near(pane_max(f32::NAN), PANE_PEEK));
}

#[test]
fn a_pane_released_near_an_end_settles_on_it_and_one_between_stays() {
    // within 28 of the peek (56): the peek
    assert!(near(settle_pane_height(60.0, 800.0), PANE_PEEK));
    assert!(near(settle_pane_height(84.0, 800.0), PANE_PEEK));
    // just outside: where it is
    assert!(near(settle_pane_height(85.0, 800.0), 85.0));
    assert!(near(settle_pane_height(300.0, 800.0), 300.0));
    // within 28 of the full height (680)
    assert!(near(settle_pane_height(660.0, 800.0), 680.0));
    assert!(near(settle_pane_height(651.0, 800.0), 651.0));
    // a drag past the ends is held first, then settled
    assert!(near(settle_pane_height(5000.0, 800.0), 680.0));
    // a height that is no height is the default
    assert!(near(settle_pane_height(-4.0, 800.0), PANE_HEIGHT_DEFAULT));
}

#[test]
fn the_pane_grows_as_the_finger_goes_up_and_the_right_dock_as_the_pointer_goes_left() {
    // grabbed at y 700 with the pane 168 tall; the finger at 600 made it 100 taller
    assert!(near(dragged_pane_height(168.0, 700.0, 600.0, 900.0), 268.0));
    assert!(near(dragged_pane_height(168.0, 700.0, 800.0, 900.0), PANE_PEEK), "down: no lower than the peek");
    assert!(near(dragged_inspector_width(320.0, 900.0, 800.0, 1400.0), 420.0));
    assert!(near(dragged_inspector_width(320.0, 900.0, 1000.0, 1400.0), 220.0));
    // a position that is no number moves nothing
    assert!(near(dragged_pane_height(168.0, f32::NAN, 10.0, 900.0), 168.0));
    assert!(near(dragged_inspector_width(320.0, 1.0, f32::NAN, 1400.0), 320.0));
}

#[test]
fn the_grip_is_a_finger_sized_strip() {
    assert!(GRIP_HEIGHT >= 24.0);
    assert!(PANE_PEEK > GRIP_HEIGHT, "the peek shows the grip and a line");
}

#[test]
fn form_factor_auto_is_a_desktop_where_no_window_is_being_built() {
    assert!(!FormFactor::Auto.is_phone());
    assert_eq!(FormFactor::Auto.resolved(), FormFactor::Desktop);
    assert_eq!(FormFactor::Phone.resolved(), FormFactor::Phone);
    assert_eq!(FormFactor::Desktop.resolved(), FormFactor::Desktop);
}
