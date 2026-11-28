//! The frame's decisions: which form the window is, how the sidebar starts and what a choice on a
//! phone does to it.

use azul::widgets::{FormFactor, SidebarState};

use super::*;

#[test]
fn the_form_variable_asks_for_a_phone_or_a_desktop_in_any_case_and_otherwise_the_window_decides() {
    assert_eq!(form_of(Some("phone")), FormFactor::Phone);
    assert_eq!(form_of(Some(" Phone ")), FormFactor::Phone);
    assert_eq!(form_of(Some("DESKTOP")), FormFactor::Desktop);
    assert_eq!(form_of(Some("tablet")), FormFactor::Auto);
    assert_eq!(form_of(Some("")), FormFactor::Auto);
    assert_eq!(form_of(None), FormFactor::Auto);
}

#[test]
fn a_chosen_form_is_the_form_whatever_the_window_and_the_platform_are() {
    for (width, touch) in [(300.0, true), (300.0, false), (1400.0, true), (1400.0, false)] {
        assert!(is_phone(FormFactor::Phone, width, touch), "{width} {touch}");
        assert!(!is_phone(FormFactor::Desktop, width, touch), "{width} {touch}");
    }
}

#[test]
fn auto_is_a_phone_only_on_a_touch_platform_in_a_window_under_600() {
    assert!(is_phone(FormFactor::Auto, 411.0, true), "412 dp");
    assert!(!is_phone(FormFactor::Auto, 411.0, false), "a narrow desktop window is no phone");
    assert!(!is_phone(FormFactor::Auto, 640.0, true), "a tablet or a landscape phone: text headers, the desktop's form");
    assert!(!is_phone(FormFactor::Auto, 1280.0, true));
    assert!(!is_phone(FormFactor::Auto, PHONE_MAX_WIDTH, true), "the line itself is the desktop's");
    assert!(!is_phone(FormFactor::Auto, 0.0, true), "a window not measured yet is not narrow");
    assert!(!is_phone(FormFactor::Auto, f32::NAN, true));
}

#[test]
fn a_phone_starts_with_the_drawer_shut_and_a_desktop_as_it_was_left() {
    assert_eq!(initial_sidebar(FormFactor::Phone, true, false), SidebarState::Rail);
    assert_eq!(initial_sidebar(FormFactor::Phone, false, false), SidebarState::Rail);
    assert_eq!(initial_sidebar(FormFactor::Desktop, false, false), SidebarState::Expanded);
    assert_eq!(initial_sidebar(FormFactor::Desktop, false, true), SidebarState::Rail);
    assert_eq!(initial_sidebar(FormFactor::Desktop, true, false), SidebarState::Expanded, "a chosen desktop is one");
    assert_eq!(initial_sidebar(FormFactor::Auto, false, false), SidebarState::Expanded);
    assert_eq!(initial_sidebar(FormFactor::Auto, false, true), SidebarState::Rail);
    assert_eq!(initial_sidebar(FormFactor::Auto, true, false), SidebarState::Rail, "a touch platform: the rail until measured");
}

#[test]
fn choosing_a_folder_closes_a_phones_drawer_and_nothing_else() {
    assert_eq!(after_choice(SidebarState::Expanded, true), SidebarState::Rail);
    assert_eq!(after_choice(SidebarState::Expanded, false), SidebarState::Expanded);
    assert_eq!(after_choice(SidebarState::Rail, true), SidebarState::Rail);
    assert_eq!(after_choice(SidebarState::Handle, true), SidebarState::Handle);
}
