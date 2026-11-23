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
