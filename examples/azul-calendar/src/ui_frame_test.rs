use super::*;

#[test]
fn the_form_variable_says_phone_or_desktop_in_any_case_else_the_window_decides() {
    assert_eq!(form_of(Some("phone")), FormFactor::Phone);
    assert_eq!(form_of(Some(" Phone ")), FormFactor::Phone);
    assert_eq!(form_of(Some("DESKTOP")), FormFactor::Desktop);
    assert_eq!(form_of(Some("tablet")), FormFactor::Auto);
    assert_eq!(form_of(Some("")), FormFactor::Auto);
    assert_eq!(form_of(None), FormFactor::Auto);
}

#[test]
fn a_window_is_a_phones_on_a_touch_platform_under_600_dp_or_when_asked() {
    // asked for
    assert!(is_phone(FormFactor::Phone, 1280.0, false));
    assert!(!is_phone(FormFactor::Desktop, 360.0, true));
    // the window decides: touch and narrow
    assert!(is_phone(FormFactor::Auto, 412.0, true));
    assert!(is_phone(FormFactor::Auto, 599.9, true));
    // a tablet, a landscape phone and a desktop window narrowed are not
    assert!(!is_phone(FormFactor::Auto, 600.0, true));
    assert!(!is_phone(FormFactor::Auto, 800.0, true));
    assert!(!is_phone(FormFactor::Auto, 360.0, false));
    // a width not known yet is no phone's by itself
    assert!(!is_phone(FormFactor::Auto, 0.0, true));
}

#[test]
fn the_sidebar_starts_as_the_rail_on_a_phone_and_as_it_was_left_on_a_desktop() {
    assert_eq!(initial_sidebar(FormFactor::Phone, true, false), SidebarState::Rail);
    assert_eq!(initial_sidebar(FormFactor::Desktop, false, false), SidebarState::Expanded);
    assert_eq!(initial_sidebar(FormFactor::Desktop, false, true), SidebarState::Rail);
    assert_eq!(initial_sidebar(FormFactor::Auto, false, false), SidebarState::Expanded);
    assert_eq!(initial_sidebar(FormFactor::Auto, false, true), SidebarState::Rail);
    // a touch platform whose width is not known yet starts as a phone does
    assert_eq!(initial_sidebar(FormFactor::Auto, true, false), SidebarState::Rail);
}

#[test]
fn choosing_something_closes_a_phones_drawer_and_leaves_a_desktops_sidebar_open() {
    assert_eq!(after_choice(SidebarState::Expanded, true), SidebarState::Rail);
    assert_eq!(after_choice(SidebarState::Expanded, false), SidebarState::Expanded);
    assert_eq!(after_choice(SidebarState::Rail, true), SidebarState::Rail);
}

#[test]
fn a_phone_starts_on_one_day_and_a_desktop_on_the_week() {
    assert_eq!(default_view_for(true), crate::views::ViewKind::Day);
    assert_eq!(default_view_for(false), crate::views::ViewKind::Week);
}
