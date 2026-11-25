use azul_css::env::{MapEnv, NoEnv};

use super::*;

#[test]
fn the_default_is_the_empty_environment_of_every_part() {
    let none = AppSettings::from_lookup(&NoEnv);
    assert_eq!(none, AppSettings::default());
    assert_eq!(none.layout, LayoutSettings::default());
    assert_eq!(none.shell, ShellSettings::default());
    assert_eq!(none.webrender, WebRenderSettings::default());
    assert_eq!(none.css, azul_css::settings::CssSettings::default());
    assert_eq!(none.core, azul_core::settings::CoreSettings::default());
}

/// The layout engine's switches are read from the SAME environment as the shell's: a variable of
/// the solver and one of the text group arrive in the carrier, where the windows and the app's
/// font manager take them from.
#[test]
fn the_layout_settings_are_built_from_the_one_environment() {
    use azul_layout::settings::DenseTextMode;

    let env = MapEnv::new()
        .with("AZ_NO_DL_PATCH", "1")
        .with("AZ_DENSE_TEXT", "verify")
        .with("AZ_TEXT_HINTING", "0")
        .with("AZ_BACKEND", "cpu");
    let settings = AppSettings::from_lookup(&env);
    assert!(!settings.layout.solver.patching);
    assert_eq!(settings.layout.solver.dense_text, DenseTextMode::Verify);
    assert!(!settings.layout.text.hinting);
    assert_eq!(settings.shell.backend.as_deref(), Some("cpu"));
    assert_eq!(settings.layout, LayoutSettings::from_lookup(&env));
}

#[test]
fn every_part_is_built_from_the_one_environment() {
    let env = MapEnv::new()
        .with("AZ_BACKEND", "cpu")
        .with("AZ_WR_ZOOM_TILES", "0");
    let settings = AppSettings::from_lookup(&env);
    assert_eq!(settings.shell.backend.as_deref(), Some("cpu"));
    assert!(!settings.webrender.zoom_tiles);
    // the rest stays what an empty environment says
    let mut expected = AppSettings::default();
    expected.shell.backend = Some(String::from("cpu"));
    expected.webrender.zoom_tiles = false;
    assert_eq!(settings, expected);
}

#[test]
fn the_css_and_core_parts_are_built_from_the_same_environment() {
    let env = MapEnv::new()
        .with("AZ_RICING", "off")
        .with("AZ_MODE", "dark")
        .with("AZ_SUPPRESS", "var_fallback");
    let settings = AppSettings::from_lookup(&env);
    assert_eq!(settings.css.ricing, azul_css::system::RicingMode::Off);
    assert_eq!(
        settings.css.theme.mode,
        Some(azul_css::system::DarkLightMode::Dark)
    );
    assert!(settings.core.diag.suppress.contains("var_fallback"));
}

#[test]
fn a_closure_is_an_environment_too() {
    let by_closure = |name: &str| (name == "AZ_HIT_DEBUG").then(|| String::from("1"));
    assert!(AppSettings::from_lookup(&by_closure).shell.hit_debug);
}

#[test]
fn the_platform_lookups_and_the_linux_switches_come_from_the_same_environment() {
    let none = AppSettings::from_lookup(&NoEnv);
    assert_eq!(none.platform, PlatformEnv::default());
    #[cfg(az_x11)]
    assert_eq!(none.linux, LinuxSettings::default());

    let env = MapEnv::new()
        .with("HOME", "/home/u")
        .with("WAYLAND_DISPLAY", "wayland-0")
        .with("AZ_NO_PACE", "1");
    let settings = AppSettings::from_lookup(&env);
    assert_eq!(settings.platform.home(), Some("/home/u"));
    assert_eq!(settings.platform.wayland_display.as_deref(), Some("wayland-0"));
    #[cfg(az_x11)]
    assert!(settings.linux.no_pace);
    // the other parts did not see them
    assert_eq!(settings.shell, ShellSettings::default());
}

#[test]
fn the_layout_group_is_built_from_the_same_environment() {
    let env = MapEnv::new()
        .with("AZ_E2E_SHOT_DIR", "/tmp/azul-shots")
        .with("AZ_NO_DL_PATCH", "1")
        .with("AZ_DEBUG", "8765");
    let settings = AppSettings::from_lookup(&env);
    assert_eq!(
        settings.layout.e2e.shot_dir,
        Some(std::path::PathBuf::from("/tmp/azul-shots"))
    );
    assert_eq!(settings.layout.e2e.debug_port, Some(8765));
    assert!(!settings.layout.solver.patching);
    // an empty environment is the layout default
    assert_eq!(
        AppSettings::from_lookup(&NoEnv).layout,
        azul_layout::settings::LayoutSettings::default()
    );
}
