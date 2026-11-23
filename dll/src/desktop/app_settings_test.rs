use azul_css::env::{MapEnv, NoEnv};

use super::*;

#[test]
fn the_default_is_the_empty_environment_of_every_part() {
    let none = AppSettings::from_lookup(&NoEnv);
    assert_eq!(none, AppSettings::default());
    assert_eq!(none.shell, ShellSettings::default());
    assert_eq!(none.render, RenderSettings::default());
    assert_eq!(none.css, azul_css::settings::CssSettings::default());
    assert_eq!(none.core, azul_core::settings::CoreSettings::default());
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
