use azul_css::env::{MapEnv, NoEnv};

use super::*;

#[test]
fn the_default_is_the_empty_environment_of_every_part() {
    let none = AppSettings::from_lookup(&NoEnv);
    assert_eq!(none, AppSettings::default());
    assert_eq!(none.layout, LayoutSettings::default());
    assert_eq!(none.shell, ShellSettings::default());
    assert_eq!(none.webrender, WebRenderSettings::default());
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
fn a_closure_is_an_environment_too() {
    let by_closure = |name: &str| (name == "AZ_HIT_DEBUG").then(|| String::from("1"));
    assert!(AppSettings::from_lookup(&by_closure).shell.hit_debug);
}
