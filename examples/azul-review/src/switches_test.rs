//! AzReview's own switches and the dictation settings they override. No window.

use std::collections::BTreeMap;

use super::*;

fn argv(args: &[&str]) -> Vec<String> {
    args.iter().map(|a| (*a).to_string()).collect()
}

#[test]
fn the_switches_are_taken_out_and_the_rest_is_left_for_the_kit() {
    let (sw, rest) = Switches::extract(argv(&[
        "folder", "--dwell-ms", "400", "--theme", "flora", "--dwell-input=any", "--silence-ms=3000", "--shot", "a.png",
    ]))
    .expect("valid switches");
    assert_eq!(sw.dwell_ms, Some(400));
    assert_eq!(sw.dwell_input, Some(Allow::Any));
    assert_eq!(sw.silence_ms, Some(3_000));
    assert_eq!(rest, argv(&["folder", "--theme", "flora", "--shot", "a.png"]));
}

#[test]
fn no_switch_leaves_everything_to_the_settings() {
    let (sw, rest) = Switches::extract(argv(&["folder"])).expect("nothing to take");
    assert_eq!(sw, Switches::default());
    assert_eq!(rest, argv(&["folder"]));
}

#[test]
fn a_switch_that_cannot_be_read_is_an_error_with_its_name() {
    for bad in [
        vec!["--dwell-ms", "soon"],
        vec!["--dwell-ms"],
        vec!["--silence-ms=-5"],
        vec!["--dwell-input", "stylus"],
        vec!["--standby", "sometimes"],
        vec!["--standby-idle-ms", "soon"],
    ] {
        let why = Switches::extract(argv(&bad)).expect_err("a switch that is wrong is refused");
        assert!(why.starts_with("--"), "{bad:?}: {why}");
    }
}

#[test]
fn the_defaults_are_a_pen_resting_one_point_two_seconds_and_ten_seconds_of_quiet() {
    let r = resolve(&BTreeMap::new(), &Switches::default());
    assert_eq!(r.allow, Allow::Pen);
    assert_eq!(r.rest_ms, 1_200);
    assert_eq!(r.silence_ms, 10_000);
    assert!(r.notes.is_empty());
}

#[test]
fn the_settings_file_is_read_and_a_switch_wins_over_it() {
    let mut values = BTreeMap::new();
    values.insert(KEY_INPUT.to_string(), "any".to_string());
    values.insert(KEY_REST.to_string(), "2000".to_string());
    values.insert(KEY_SILENCE.to_string(), "20000".to_string());
    let r = resolve(&values, &Switches::default());
    assert_eq!((r.allow, r.rest_ms, r.silence_ms), (Allow::Any, 2_000, 20_000));

    let sw = Switches {
        dwell_ms: Some(500),
        dwell_input: Some(Allow::Off),
        silence_ms: Some(5_000),
        ..Switches::default()
    };
    let r = resolve(&values, &sw);
    assert_eq!((r.allow, r.rest_ms, r.silence_ms), (Allow::Off, 500, 5_000));
}

#[test]
fn a_value_nobody_can_mean_is_capped_and_said_not_obeyed_and_not_fatal() {
    let mut values = BTreeMap::new();
    values.insert(KEY_REST.to_string(), "5".to_string());
    values.insert(KEY_SILENCE.to_string(), "9999999999".to_string());
    values.insert(KEY_INPUT.to_string(), "stylus".to_string());
    let r = resolve(&values, &Switches::default());
    assert_eq!(r.rest_ms, dwell::MIN_REST_MS);
    assert_eq!(r.silence_ms, dictation::MAX_SILENCE_MS);
    assert_eq!(r.allow, Allow::Pen, "an input nobody knows is the default");
    assert_eq!(r.notes.len(), 3, "each change is told: {:?}", r.notes);
}

#[test]
fn a_value_that_is_no_number_is_the_default() {
    let mut values = BTreeMap::new();
    values.insert(KEY_REST.to_string(), "quickly".to_string());
    let r = resolve(&values, &Switches::default());
    assert_eq!(r.rest_ms, dwell::DEFAULT_REST_MS);
    assert_eq!(r.notes.len(), 1);
}

// ---- the standby of the microphone ----

#[test]
fn the_standby_switches_are_taken_out_like_the_others() {
    let (sw, rest) = Switches::extract(argv(&["--standby", "off", "folder", "--standby-idle-ms=8000"])).expect("valid");
    assert_eq!(sw.standby, Some(Standby::Off));
    assert_eq!(sw.standby_idle_ms, Some(8_000));
    assert_eq!(rest, argv(&["folder"]));
    let (sw, _) = Switches::extract(argv(&["--standby=ALWAYS"])).expect("any case");
    assert_eq!(sw.standby, Some(Standby::Always));
}

#[test]
fn the_microphone_is_ready_when_the_pen_is_near_for_half_a_minute_unless_said_otherwise() {
    let r = resolve(&BTreeMap::new(), &Switches::default());
    assert_eq!(r.standby, Standby::Near);
    assert_eq!(r.standby_idle_ms, standby::DEFAULT_IDLE_MS);
    assert!(r.notes.is_empty(), "{:?}", r.notes);
}

#[test]
fn the_settings_file_says_the_standby_and_a_switch_wins_over_it() {
    let mut values = BTreeMap::new();
    values.insert(KEY_STANDBY.to_string(), "always".to_string());
    values.insert(KEY_STANDBY_IDLE.to_string(), "120000".to_string());
    let r = resolve(&values, &Switches::default());
    assert_eq!((r.standby, r.standby_idle_ms), (Standby::Always, 120_000));

    let sw = Switches {
        standby: Some(Standby::Off),
        standby_idle_ms: Some(10_000),
        ..Switches::default()
    };
    let r = resolve(&values, &sw);
    assert_eq!((r.standby, r.standby_idle_ms), (Standby::Off, 10_000));
}

#[test]
fn a_standby_nobody_can_mean_is_the_default_capped_and_said_not_obeyed_and_not_fatal() {
    let mut values = BTreeMap::new();
    values.insert(KEY_STANDBY.to_string(), "sometimes".to_string());
    values.insert(KEY_STANDBY_IDLE.to_string(), "10".to_string());
    let r = resolve(&values, &Switches::default());
    assert_eq!(r.standby, Standby::Near, "a choice nobody knows is the default");
    assert_eq!(r.standby_idle_ms, standby::MIN_IDLE_MS);
    assert_eq!(r.notes.len(), 2, "each change is told: {:?}", r.notes);
    assert!(r.notes.iter().any(|n| n.contains(KEY_STANDBY)), "{:?}", r.notes);
}

// ---- the folder to review ----

use std::path::{Path, PathBuf};

fn is_dir_of(dirs: &'static [&'static str]) -> impl Fn(&Path) -> bool {
    move |p| dirs.iter().any(|d| Path::new(d) == p)
}

#[test]
fn the_folder_named_on_the_command_line_is_the_one_reviewed() {
    let root = pick_root(
        Some(PathBuf::from("/named")),
        Some("/last"),
        true,
        is_dir_of(&["/last", "/named"]),
    );
    assert_eq!(root, PathBuf::from("/named"));
}

#[test]
fn without_one_the_folder_reviewed_last_comes_back_if_it_is_still_a_folder() {
    let root = pick_root(None, Some("/last"), true, is_dir_of(&["/last"]));
    assert_eq!(root, PathBuf::from("/last"));
}

#[test]
fn a_folder_that_is_gone_is_not_taken_and_the_current_folder_is_never_the_default() {
    // The folder the process was started in (the terminal's, the Finder's `/`, a launcher's) is no choice of
    // the user's: with nothing named and nothing remembered there is no folder, and the window says so.
    let gone = pick_root(None, Some("/last"), true, is_dir_of(&[]));
    assert_eq!(gone, PathBuf::new(), "a folder that is gone is not taken, and nothing takes its place");
    assert_eq!(pick_root(None, None, true, |_| true), PathBuf::new());
    assert_eq!(pick_root(None, None, false, |_| true), PathBuf::new());
}

#[test]
fn a_fixture_run_never_takes_the_folder_the_user_reviewed_last() {
    let root = pick_root(None, Some("/last"), false, is_dir_of(&["/last"]));
    assert_eq!(root, PathBuf::new());
}

#[test]
fn an_empty_remembered_folder_is_no_folder() {
    assert_eq!(pick_root(None, Some("  "), true, |_| true), PathBuf::new());
    assert_eq!(pick_root(None, Some(""), true, |_| true), PathBuf::new());
}

#[test]
fn the_agent_switches_are_taken_out_like_the_others() {
    let (sw, rest) = Switches::extract(argv(&["f", "--agent", "mock", "--agent-delay-ms=40", "--agent-auto", "off"])).expect("valid");
    assert_eq!(sw.agent, Some(crate::agent_backend::BackendKind::Mock));
    assert_eq!(sw.agent_delay_ms, Some(40));
    assert_eq!(sw.agent_auto, Some(false));
    assert_eq!(rest, argv(&["f"]));
    let (sw, _) = Switches::extract(argv(&["--agent=claude-print", "--agent-auto=on"])).expect("valid");
    assert_eq!(sw.agent, Some(crate::agent_backend::BackendKind::ClaudePrint));
    assert_eq!(sw.agent_auto, Some(true));
}

#[test]
fn an_agent_switch_that_cannot_be_read_is_refused_with_the_choices() {
    let why = Switches::extract(argv(&["--agent", "gpt"])).expect_err("no such backend");
    assert!(why.contains("--agent") && why.contains("mock") && why.contains("claude-print"), "{why}");
    assert!(Switches::extract(argv(&["--agent-delay-ms", "soon"])).is_err());
    let why = Switches::extract(argv(&["--agent-auto", "maybe"])).expect_err("on or off");
    assert!(why.contains("on") && why.contains("off"), "{why}");
}

#[test]
fn the_agent_asks_by_itself_unless_a_setting_or_a_switch_says_not_and_the_switch_wins() {
    let none = BTreeMap::new();
    let r = resolve(&none, &Switches::default());
    assert_eq!((r.agent, r.agent_delay_ms, r.agent_auto), (None, 800, true));
    let mut values = BTreeMap::new();
    values.insert(KEY_ASK_AGENT.to_string(), "off".to_string());
    assert!(!resolve(&values, &Switches::default()).agent_auto);
    let sw = Switches { agent_auto: Some(true), ..Switches::default() };
    assert!(resolve(&values, &sw).agent_auto, "the switch over the setting");
    values.insert(KEY_ASK_AGENT.to_string(), "sometimes".to_string());
    let r = resolve(&values, &Switches::default());
    assert!(r.agent_auto, "a setting that is neither on nor off is the default");
    assert!(r.notes.iter().any(|n| n.contains(KEY_ASK_AGENT)), "and the log is told: {:?}", r.notes);
}

#[test]
fn the_symbol_index_is_built_unless_a_setting_or_a_switch_says_not_and_the_switch_wins() {
    let (sw, rest) = Switches::extract(argv(&["f", "--symbols", "off"])).expect("valid");
    assert_eq!(sw.symbols, Some(false));
    assert_eq!(rest, argv(&["f"]));
    assert_eq!(Switches::extract(argv(&["--symbols=on"])).expect("valid").0.symbols, Some(true));
    let why = Switches::extract(argv(&["--symbols", "maybe"])).expect_err("on or off");
    assert!(why.contains("--symbols") && why.contains("on") && why.contains("off"), "{why}");

    let none = BTreeMap::new();
    assert!(resolve(&none, &Switches::default()).symbols, "built by default");
    let mut values = BTreeMap::new();
    values.insert(KEY_SYMBOLS.to_string(), "off".to_string());
    assert!(!resolve(&values, &Switches::default()).symbols);
    assert!(resolve(&values, &Switches { symbols: Some(true), ..Switches::default() }).symbols, "the switch over the setting");
    values.insert(KEY_SYMBOLS.to_string(), "later".to_string());
    let r = resolve(&values, &Switches::default());
    assert!(r.symbols && r.notes.iter().any(|n| n.contains(KEY_SYMBOLS)), "{:?}", r.notes);
}

// ---- how long a pause ends an utterance (the engine's end-point) ----

#[test]
fn the_pause_that_ends_an_utterance_is_a_switch_a_setting_and_by_default_the_engine_s_own() {
    let (sw, rest) = Switches::extract(argv(&["folder", "--endpoint-ms", "900"])).expect("valid");
    assert_eq!(sw.endpoint_ms, Some(900));
    assert_eq!(rest, argv(&["folder"]));
    assert!(Switches::extract(argv(&["--endpoint-ms", "later"])).expect_err("no number").contains("--endpoint-ms"));
    assert!(HELP.contains("--endpoint-ms"));

    // nothing says: 0, which is the engine's own (600 ms)
    assert_eq!(resolve(&BTreeMap::new(), &Switches::default()).endpoint_ms, 0);
    // the settings file's value
    let mut values = BTreeMap::new();
    values.insert(KEY_ENDPOINT.to_string(), "1000".to_string());
    assert_eq!(resolve(&values, &Switches::default()).endpoint_ms, 1_000);
    // a switch over the settings file's value
    assert_eq!(resolve(&values, &Switches { endpoint_ms: Some(800), ..Switches::default() }).endpoint_ms, 800);
}

#[test]
fn a_pause_that_no_dictation_can_mean_is_held_to_what_one_can_and_said() {
    let r = resolve(&BTreeMap::new(), &Switches { endpoint_ms: Some(50), ..Switches::default() });
    assert_eq!(r.endpoint_ms, u32::try_from(dictation::MIN_ENDPOINT_MS).expect("small"));
    assert!(r.notes.iter().any(|n| n.contains("50")), "{:?}", r.notes);
    let r = resolve(&BTreeMap::new(), &Switches { endpoint_ms: Some(60_000), ..Switches::default() });
    assert_eq!(u64::from(r.endpoint_ms), dictation::MAX_ENDPOINT_MS);
    let mut values = BTreeMap::new();
    values.insert(KEY_ENDPOINT.to_string(), "soon".to_string());
    let r = resolve(&values, &Switches::default());
    assert_eq!(r.endpoint_ms, 0, "a setting that is no number is the engine's own");
    assert!(r.notes.iter().any(|n| n.contains(KEY_ENDPOINT)), "{:?}", r.notes);
}
