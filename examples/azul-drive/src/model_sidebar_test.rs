use crate::model::{clamped_preview_size, Density, Settings, SidebarMode, PREVIEW_SIZE_MAX, TOUCH_ROW_PX};

/// Every app starts as the rail: a settings file that says nothing about the sidebar (the files of
/// the builds before it, which kept a yes or no for the navigation pane) opens the rail.
#[test]
fn a_settings_file_without_the_sidebar_starts_as_the_rail() {
    let old: Settings = serde_json::from_str(r#"{"navigation_pane": true, "preview_pane": false}"#)
        .expect("an old file still reads");
    assert_eq!(old.sidebar, SidebarMode::Rail);
    assert!(!old.preview_pane, "what the file does say is kept");
    assert_eq!(Settings::default().sidebar, SidebarMode::Rail);
}

/// The three states round trip by their names, so a file written by one window reads in another.
#[test]
fn the_sidebar_states_round_trip_by_name() {
    for (mode, name) in [
        (SidebarMode::Expanded, "expanded"),
        (SidebarMode::Rail, "rail"),
        (SidebarMode::Handle, "handle"),
    ] {
        let settings = Settings {
            sidebar: mode,
            ..Settings::default()
        };
        let text = serde_json::to_string(&settings).expect("settings write");
        assert!(text.contains(&format!("\"sidebar\":\"{name}\"")), "{text}");
        let back: Settings = serde_json::from_str(&text).expect("settings read");
        assert_eq!(back.sidebar, mode);
    }
}

/// The View tab's Navigation pane opens the sidebar and closes it to the rail (never to the
/// handle, which only the sidebar's own control asks for).
#[test]
fn the_navigation_pane_toggle_swaps_expanded_and_rail() {
    let mut settings = Settings::default();
    assert!(!settings.navigation_pane());
    settings.toggle_navigation_pane();
    assert_eq!(settings.sidebar, SidebarMode::Expanded);
    assert!(settings.navigation_pane());
    settings.toggle_navigation_pane();
    assert_eq!(settings.sidebar, SidebarMode::Rail);
    settings.sidebar = SidebarMode::Handle;
    settings.toggle_navigation_pane();
    assert_eq!(settings.sidebar, SidebarMode::Expanded, "from the handle it opens");
}

/// A pane size from the settings file is capped, never trusted: a number that is no size is the
/// widget's default (0), a size beyond the limit is the limit.
#[test]
fn a_preview_size_from_the_file_is_capped() {
    assert_eq!(clamped_preview_size(0.0), 0.0);
    assert_eq!(clamped_preview_size(320.0), 320.0);
    assert_eq!(clamped_preview_size(-5.0), 0.0);
    assert_eq!(clamped_preview_size(f32::NAN), 0.0);
    assert_eq!(clamped_preview_size(f32::INFINITY), 0.0);
    assert_eq!(clamped_preview_size(1.0e9), PREVIEW_SIZE_MAX);
}

/// Where a finger points (a phone) the rows of the folder view are 48 px, whatever density the user chose for the
/// pointer of a desktop; a desktop keeps the density's own.
#[test]
fn a_touch_screens_rows_are_a_fingers_size_whatever_the_density() {
    assert_eq!(TOUCH_ROW_PX, 48.0);
    for density in [Density::Compact, Density::Normal, Density::Comfortable] {
        assert_eq!(density.row_px_for(true), 48.0);
        assert_eq!(density.row_px_for(false), density.row_px());
    }
}
