//! What a platform's style discovery may ask of a subprocess: a command-line answer to an accessibility flag is wanted
//! only where the native query did not run; and the line `App::create` logs for each of its steps. Pure.

use azul_css::dynamic_selector::BoolCondition;

use super::*;

/// The native query answered (a `False` is an answer as much as a `True`): the command line is not asked. It is asked
/// only where the native query did not run (the library did not load, the class was not there) and left the flag at its
/// default. This is what the two `defaults read` processes of `App::create` (12 ms of its first 61 on a Mac, on the
/// window's thread) were: a fallback that ran for every user who has not turned Reduce motion or Increase contrast on,
/// and got the answer the native query had given.
///
/// Mutation: a native `False` that is asked again; a native `True` that is asked again; a native query that did not run and
/// is not asked.
#[test]
fn the_command_line_is_asked_only_where_the_native_query_did_not_answer() {
    assert!(!cli_fallback_wanted(true, BoolCondition::False), "the native query said no: no process");
    assert!(!cli_fallback_wanted(true, BoolCondition::True), "the native query said yes: no process");
    assert!(cli_fallback_wanted(false, BoolCondition::False), "the native query did not run: the command line is the only answer");
    assert!(!cli_fallback_wanted(false, BoolCondition::True), "an answer already there is kept");
}

/// The line of a step of `App::create`: its name and its time in microseconds, so that a script can read the number of
/// each step.
///
/// Mutation: the time in the wrong unit; the name missing.
#[test]
fn a_step_of_app_create_is_logged_with_its_name_and_its_microseconds() {
    assert_eq!(step_line("settings", 1_234), "[app] App::create: settings 1234 us");
    assert_eq!(step_line("system_style", 0), "[app] App::create: system_style 0 us");
}

/// The steps measure the time since the one before: the first since the start, each next since the last.
///
/// Mutation: every step measured from the start; the clock not moved on after a step.
#[test]
fn each_step_is_measured_from_the_one_before() {
    let mut steps = CreateSteps::at(1_000);
    assert_eq!(steps.lap_at("one", 1_300), "[app] App::create: one 300 us");
    assert_eq!(steps.lap_at("two", 1_350), "[app] App::create: two 50 us");
    assert_eq!(steps.lap_at("three", 1_350), "[app] App::create: three 0 us", "a clock that did not move");
    assert_eq!(steps.lap_at("four", 1_000), "[app] App::create: four 0 us", "a clock that went back is no time");
    assert_eq!(steps.total_at(2_000), 1_000, "the whole is from the start");
}
