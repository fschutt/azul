use azul_css::env::{MapEnv, NoEnv};

use super::*;

#[test]
fn the_default_is_the_empty_environment_of_every_part() {
    let none = AppSettings::from_lookup(&NoEnv);
    assert_eq!(none, AppSettings::default());
    assert_eq!(none.shell, ShellSettings::default());
    assert_eq!(none.render, RenderSettings::default());
}

#[test]
fn every_part_is_built_from_the_one_environment() {
    let env = MapEnv::new()
        .with("AZ_BACKEND", "cpu")
        .with("AZ_WR_ZOOM_TILES", "0");
    let settings = AppSettings::from_lookup(&env);
    assert_eq!(settings.shell.backend.as_deref(), Some("cpu"));
    assert!(!settings.render.zoom_tiles);
    // the rest stays what an empty environment says
    let mut expected = AppSettings::default();
    expected.shell.backend = Some(String::from("cpu"));
    expected.render.zoom_tiles = false;
    assert_eq!(settings, expected);
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
