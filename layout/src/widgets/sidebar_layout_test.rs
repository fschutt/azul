//! The numbers of the sidebar family: widths in each state, the phone's pushing drawer, the window
//! controls' room, the Preview's limits and where its pane settles.

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
fn the_rail_is_18_wide_on_a_desktop_and_on_a_phone() {
    for form in [FormFactor::Desktop, FormFactor::Phone] {
        assert!(near(column_width(SidebarState::Rail, form, 0.0, 0.0).px, 18.0), "{form:?}");
    }
    assert!(near(RAIL_WIDTH, 18.0));
}

#[test]
fn the_window_controls_overhang_the_rail_they_do_not_widen_it() {
    // the macOS traffic lights are 78 px of room: the rail stays 18 and the tab row clears the rest
    for form in [FormFactor::Desktop, FormFactor::Phone] {
        for lights in [78.0, f32::NAN, -30.0, 0.0] {
            assert!(near(column_width(SidebarState::Rail, form, 0.0, lights).px, 18.0), "{form:?} {lights}");
        }
    }
    assert!(near(header_left_pad(78.0, RAIL_WIDTH), 60.0));
}

#[test]
fn a_drag_inward_of_24_px_that_is_mostly_horizontal_opens_the_drawer() {
    assert!(swipe_opens(24.0, 0.0));
    assert!(swipe_opens(40.0, 15.0));
    assert!(!swipe_opens(23.9, 0.0), "too short");
    assert!(!swipe_opens(40.0, 25.0), "mostly vertical: that is a scroll of the rail");
    assert!(!swipe_opens(-40.0, 0.0), "outward opens nothing");
    assert!(!swipe_opens(f32::NAN, 0.0) && !swipe_opens(40.0, f32::INFINITY));
}

// what is drawn is 18 px with 13 px icons; what is hit reaches 18 px further inward, and a drag
// inward of 24 px opens the drawer: checked when this compiles
const _: () = assert!(RAIL_ICON_PX >= 12.0 && RAIL_ICON_PX <= 14.0);
const _: () = assert!(RAIL_SLOP >= 16.0 && RAIL_SLOP <= 20.0);
const _: () = assert!(RAIL_ICON_PX < RAIL_WIDTH);
const _: () = assert!(EDGE_SWIPE_PX > 8.0 && EDGE_SWIPE_PX < 64.0);
const _: () = assert!(RAIL_BUTTON_PHONE >= 44.0, "a finger needs 44 px along the rail");

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
    assert!(near(col.in_frame(10.0), 18.0));
    assert!(near(col.in_frame(1000.0), 18.0));
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
    // the 18 px rail covers little of them: the tab row clears the rest
    assert!(near(header_left_pad(78.0, 18.0), 60.0));
    // an expanded sidebar wider than the controls: nothing left to clear
    assert!(near(header_left_pad(78.0, 78.0), 0.0));
    assert!(near(header_left_pad(78.0, 240.0), 0.0));
    assert!(near(header_left_pad(0.0, 54.0), 0.0));
    assert!(near(header_left_pad(f32::NAN, f32::NAN), 0.0));
}

// ---- the Preview ----

#[test]
fn the_preview_docks_right_on_a_desktop_and_at_the_bottom_on_a_phone() {
    assert_eq!(PreviewDock::Auto.resolved(FormFactor::Desktop), PreviewDock::Right);
    assert_eq!(PreviewDock::Auto.resolved(FormFactor::Phone), PreviewDock::Bottom);
    // a dock the app names stays
    assert_eq!(PreviewDock::Right.resolved(FormFactor::Phone), PreviewDock::Right);
    assert_eq!(PreviewDock::Bottom.resolved(FormFactor::Desktop), PreviewDock::Bottom);
}

#[test]
fn the_right_dock_s_width_stays_between_200_and_60_percent_and_leaves_the_body_240() {
    // a 1200 px container: 60 % is 720, the body keeps 240, so the limit is 720
    assert!(near(clamp_preview_width(320.0, 1200.0), 320.0));
    assert!(near(clamp_preview_width(100.0, 1200.0), 200.0));
    assert!(near(clamp_preview_width(2000.0, 1200.0), 720.0));
    // a 600 px container: 60 % is 360, but the body keeps 240 so 360 is the limit (600 - 240)
    assert!(near(clamp_preview_width(2000.0, 600.0), 360.0));
    // a 400 px container is too narrow for both limits: the minimum wins
    assert!(near(clamp_preview_width(300.0, 400.0), 200.0));
}

#[test]
fn a_dock_width_that_is_no_number_is_the_default_held_to_the_limits() {
    assert!(near(clamp_preview_width(f32::NAN, 1200.0), PREVIEW_WIDTH_DEFAULT));
    assert!(near(clamp_preview_width(0.0, 1200.0), PREVIEW_WIDTH_DEFAULT));
    assert!(near(clamp_preview_width(f32::INFINITY, 1200.0), PREVIEW_WIDTH_DEFAULT));
    // a negative width is no width the pane can have: the minimum
    assert!(near(clamp_preview_width(-5.0, 1200.0), PREVIEW_WIDTH_MIN));
    // an unknown container: the width is only held to the minimum
    assert!(near(clamp_preview_width(320.0, f32::NAN), 320.0));
    assert!(near(clamp_preview_width(120.0, 0.0), 200.0));
}

#[test]
fn the_bottom_pane_stays_between_the_collapsed_bar_and_85_percent_of_its_container() {
    assert!(near(pane_max(800.0), 680.0));
    assert!(near(clamp_pane_height(168.0, 800.0), 168.0));
    assert!(near(clamp_pane_height(10.0, 800.0), PANE_COLLAPSED));
    assert!(near(clamp_pane_height(5000.0, 800.0), 680.0));
    assert!(near(clamp_pane_height(f32::NAN, 800.0), PANE_HEIGHT_DEFAULT));
    // a container too short for the bar still gets the bar
    assert!(near(pane_max(10.0), PANE_COLLAPSED));
    // a container that is not known puts no limit: the DOM is being built
    assert!(pane_max(f32::NAN) > 1.0e6 && pane_max(f32::INFINITY) > 1.0e6);
    assert!(near(clamp_pane_height(300.0, f32::NAN), 300.0));
    // a negative height is the collapsed bar, 0 and no number the default (the peek)
    assert!(near(clamp_pane_height(-30.0, 800.0), PANE_COLLAPSED));
    assert!(near(clamp_pane_height(0.0, 800.0), PANE_HEIGHT_DEFAULT));
    assert!(near(clamp_pane_height(f32::INFINITY, 800.0), PANE_HEIGHT_DEFAULT));
}

#[test]
fn the_bottom_pane_has_four_detents_collapsed_peek_half_full() {
    let d = pane_detents(800.0);
    assert!(near(d[0], 30.0) && near(d[1], 130.0) && near(d[2], 400.0) && near(d[3], 680.0), "{d:?}");
    // a container that is not known: the two that need it are no numbers
    let u = pane_detents(f32::NAN);
    assert!(near(u[0], 30.0) && near(u[1], 130.0) && u[2].is_nan() && u[3].is_nan(), "{u:?}");
    assert!(near(pane_half(800.0).unwrap_or(0.0), 400.0));
    assert!(pane_half(f32::NAN).is_none());
    // a default pane is the peek
    assert!(near(PANE_HEIGHT_DEFAULT, PANE_PEEK));
}

#[test]
fn a_pane_released_near_a_detent_settles_on_it_and_one_between_stays() {
    // within 28 of the collapsed bar (30)
    assert!(near(settle_pane_height(40.0, 800.0), PANE_COLLAPSED));
    assert!(near(settle_pane_height(58.0, 800.0), PANE_COLLAPSED));
    assert!(near(settle_pane_height(59.0, 800.0), 59.0));
    // the peek (130)
    assert!(near(settle_pane_height(110.0, 800.0), PANE_PEEK));
    assert!(near(settle_pane_height(158.0, 800.0), PANE_PEEK));
    assert!(near(settle_pane_height(160.0, 800.0), 160.0));
    // the half (400) and the full height (680)
    assert!(near(settle_pane_height(380.0, 800.0), 400.0));
    assert!(near(settle_pane_height(660.0, 800.0), 680.0));
    assert!(near(settle_pane_height(651.0, 800.0), 651.0));
    // between the detents: where it was left
    assert!(near(settle_pane_height(250.0, 800.0), 250.0));
    // a drag past the ends is held first, then settled
    assert!(near(settle_pane_height(5000.0, 800.0), 680.0));
    // no height is the default (the peek), a negative one the collapsed bar
    assert!(near(settle_pane_height(0.0, 800.0), PANE_HEIGHT_DEFAULT));
    assert!(near(settle_pane_height(-4.0, 800.0), PANE_COLLAPSED));
    // a container that is not known: only the bar and the peek attract
    assert!(near(settle_pane_height(120.0, f32::NAN), PANE_PEEK));
    assert!(near(settle_pane_height(300.0, f32::NAN), 300.0));
}

#[test]
fn a_tap_on_the_grip_toggles_the_peek_and_the_half() {
    assert!(near(toggle_pane_height(PANE_PEEK, 800.0), 400.0), "peek: up to the half");
    assert!(near(toggle_pane_height(400.0, 800.0), PANE_PEEK), "half: back to the peek");
    assert!(near(toggle_pane_height(680.0, 800.0), PANE_PEEK), "full: back to the peek");
    assert!(near(toggle_pane_height(PANE_COLLAPSED, 800.0), PANE_PEEK), "the bar: up to the peek");
    assert!(near(toggle_pane_height(250.0, 800.0), 400.0), "between the peek and the half: up to the half");
    assert!(near(toggle_pane_height(PANE_PEEK, f32::NAN), PANE_PEEK * 2.5), "no container: 2.5 peeks");
    // a container too short for the half: the toggle still ends on a number the pane can have
    let t = toggle_pane_height(PANE_PEEK, 200.0);
    assert!(t >= PANE_COLLAPSED && t <= pane_max(200.0), "{t}");
}

#[test]
fn the_pane_grows_as_the_finger_goes_up_and_the_right_dock_as_the_pointer_goes_left() {
    // grabbed at y 700 with the pane 168 tall; the finger at 600 made it 100 taller
    assert!(near(dragged_pane_height(168.0, 700.0, 600.0, 900.0), 268.0));
    assert!(near(dragged_pane_height(168.0, 700.0, 900.0, 900.0), PANE_COLLAPSED), "down: no lower than the bar");
    assert!(near(dragged_preview_width(320.0, 900.0, 800.0, 1400.0), 420.0));
    assert!(near(dragged_preview_width(320.0, 900.0, 1000.0, 1400.0), 220.0));
    // a position that is no number moves nothing
    assert!(near(dragged_pane_height(168.0, f32::NAN, 10.0, 900.0), 168.0));
    assert!(near(dragged_preview_width(320.0, 1.0, f32::NAN, 1400.0), 320.0));
}

// the grip is a finger-sized strip and the collapsed bar is that strip: checked when this compiles
const _: () = assert!(GRIP_HEIGHT >= 24.0);
const _: () = assert!(PANE_COLLAPSED >= GRIP_HEIGHT);
const _: () = assert!(PANE_PEEK > PANE_COLLAPSED);

#[test]
fn form_factor_auto_is_a_desktop_where_no_window_is_being_built() {
    assert!(!FormFactor::Auto.is_phone());
    assert_eq!(FormFactor::Auto.resolved(), FormFactor::Desktop);
    assert_eq!(FormFactor::Phone.resolved(), FormFactor::Phone);
    assert_eq!(FormFactor::Desktop.resolved(), FormFactor::Desktop);
}

// ---- the keys on the grip ----

#[test]
fn an_arrow_key_moves_the_pane_by_16_and_with_ctrl_by_64() {
    // a right dock grows to the left, a bottom pane grows up
    assert!(near(step_size(PreviewDock::Right, GripKey::Grow, 320.0, false, 1200.0, 700.0), 336.0));
    assert!(near(step_size(PreviewDock::Right, GripKey::Shrink, 320.0, false, 1200.0, 700.0), 304.0));
    assert!(near(step_size(PreviewDock::Bottom, GripKey::Grow, 168.0, false, 400.0, 800.0), 184.0));
    assert!(near(step_size(PreviewDock::Bottom, GripKey::Shrink, 168.0, true, 400.0, 800.0), 104.0));
}

#[test]
fn home_and_end_go_to_the_ends_of_the_range() {
    assert!(near(step_size(PreviewDock::Right, GripKey::ToMin, 320.0, false, 1200.0, 700.0), PREVIEW_WIDTH_MIN));
    assert!(near(step_size(PreviewDock::Right, GripKey::ToMax, 320.0, false, 1200.0, 700.0), 720.0));
    assert!(near(step_size(PreviewDock::Bottom, GripKey::ToMin, 300.0, false, 400.0, 800.0), PANE_COLLAPSED));
    assert!(near(step_size(PreviewDock::Bottom, GripKey::ToMax, 300.0, false, 400.0, 800.0), 680.0));
}

#[test]
fn a_key_never_takes_the_pane_past_its_limits() {
    assert!(near(step_size(PreviewDock::Right, GripKey::Shrink, 200.0, true, 1200.0, 700.0), PREVIEW_WIDTH_MIN));
    assert!(near(step_size(PreviewDock::Bottom, GripKey::Shrink, 56.0, true, 400.0, 800.0), PANE_COLLAPSED));
    assert!(near(step_size(PreviewDock::Bottom, GripKey::Grow, 680.0, true, 400.0, 800.0), 680.0));
}
