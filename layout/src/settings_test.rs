use azul_css::env::{MapEnv, NoEnv};

use super::*;

#[test]
fn the_default_is_what_an_empty_environment_gives() {
    assert_eq!(LayoutSettings::default(), LayoutSettings::from_lookup(&NoEnv));
}

#[test]
fn every_group_reads_its_own_variables_and_only_those() {
    let env = MapEnv::new()
        .with("AZ_NO_DL_PATCH", "1")
        .with("TEXTDBG", "1");
    let settings = LayoutSettings::from_lookup(&env);
    assert_eq!(
        settings.solver,
        SolverSettings {
            patching: false,
            ..SolverSettings::default()
        }
    );
    assert_eq!(
        settings.text,
        TextSettings {
            trace_text: true,
            ..TextSettings::default()
        }
    );
}

#[test]
fn the_groups_are_the_ones_the_modules_define() {
    let settings = LayoutSettings::default();
    assert_eq!(settings.solver, solver::SolverSettings::default());
    assert_eq!(settings.text, text::TextSettings::default());
}
