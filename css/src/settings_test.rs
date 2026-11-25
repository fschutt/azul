use super::*;
use crate::env::{MapEnv, NoEnv};

#[test]
fn an_empty_environment_is_the_default_settings() {
    assert_eq!(CssSettings::from_lookup(&NoEnv), CssSettings::default());
    assert_eq!(CssSettings::default().ricing, RicingMode::Default);
    assert_eq!(CssSettings::default().theme, ThemeEnv::default());
    assert_eq!(SystemEnv::from_lookup(&NoEnv), SystemEnv::default());
}

#[test]
fn the_variables_are_read_by_name_and_trimmed_as_before() {
    let env = MapEnv::new()
        .with("AZ_RICING", "off")
        .with("AZ_THEME", " xyz:pink ")
        .with("AZ_MODE", "dark");
    let settings = CssSettings::from_lookup(&env);
    assert_eq!(settings.ricing, RicingMode::Off);
    assert_eq!(settings.theme.theme.as_deref(), Some("xyz:pink"));
    assert_eq!(
        settings.theme.mode,
        Some(crate::system::DarkLightMode::Dark)
    );
    assert!(settings.theme.warnings.is_empty());
}

#[test]
fn the_deprecated_theme_alias_still_pins_the_mode_with_a_warning() {
    let env = MapEnv::new().with("AZ_THEME", "light");
    let theme = ThemeEnv::from_lookup(&env);
    assert_eq!(theme.theme, None);
    assert_eq!(theme.mode, Some(crate::system::DarkLightMode::Light));
    assert_eq!(theme.warnings.len(), 1, "{:?}", theme.warnings);
}

#[test]
fn the_session_variables_are_data() {
    let env = MapEnv::new()
        .with("XDG_CURRENT_DESKTOP", "KDE")
        .with("DESKTOP_SESSION", "plasma")
        .with("SWAYSOCK", "")
        .with("LANG", "de_DE.UTF-8")
        .with("LC_ALL", "fr_FR");
    let sys = SystemEnv::from_lookup(&env);
    assert_eq!(sys.current_desktop.as_deref(), Some("KDE"));
    assert_eq!(sys.desktop_session.as_deref(), Some("plasma"));
    assert!(sys.sway, "a marker that is set to an empty value is set");
    assert!(!sys.i3 && !sys.hyprland && !sys.gnome_session && !sys.kde_session);
    assert_eq!(
        sys.languages,
        ["fr_FR", "de_DE.UTF-8"],
        "LC_ALL before LANG"
    );
}
