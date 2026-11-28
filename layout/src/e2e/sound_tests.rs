//! The system-sound ops (`assert_sound`, `assert_no_sound`), driven end to end through the real
//! dispatcher on a headless window: an app command asks for sounds with `play_system_sound` and
//! `set_sounds_muted` exactly as a widget callback does, the window's `SoundManager` decides and
//! records them, and the ops read the recording. The scenarios use no route of a platform, so
//! they run unchanged on every host.

use azul_core::{
    events::CustomE2eOpResult,
    refany::RefAny,
    resources::AppConfig,
    sound::{route, SoundConfig, SoundPlatform, SystemSound},
};
use azul_css::{system::AudioMetrics, AzString};
use azul_layout::callbacks::{CallbackInfo, E2eCommandCallback, E2eCommandCallbackType};

use super::{runner::run_e2e_test_with_app, E2eTest, E2eTestResult};

fn answer(json: serde_json::Value) -> CustomE2eOpResult {
    CustomE2eOpResult {
        handled: true,
        json: AzString::from(json.to_string()),
    }
}

/// `{"op": "play", "sound": "TabChange", "times": 3}`: asks for the sound `times` times (once
/// when it is absent), like a callback that fires per page of a fling.
extern "C" fn play(_data: RefAny, mut info: CallbackInfo, _name: AzString, args: AzString) -> CustomE2eOpResult {
    let args: serde_json::Value = serde_json::from_str(args.as_str()).unwrap_or(serde_json::Value::Null);
    let Some(sound) = args
        .get("sound")
        .and_then(serde_json::Value::as_str)
        .and_then(SystemSound::from_name)
    else {
        return answer(serde_json::json!({ "success": false, "error": "play needs a known `sound`" }));
    };
    let times = args.get("times").and_then(serde_json::Value::as_u64).unwrap_or(1);
    for _ in 0..times {
        info.play_system_sound(sound);
    }
    answer(serde_json::json!({ "success": true }))
}

/// `{"op": "mute", "muted": true}`: `set_sounds_muted`.
extern "C" fn mute(_data: RefAny, mut info: CallbackInfo, _name: AzString, args: AzString) -> CustomE2eOpResult {
    let args: serde_json::Value = serde_json::from_str(args.as_str()).unwrap_or(serde_json::Value::Null);
    info.set_sounds_muted(args.get("muted").and_then(serde_json::Value::as_bool).unwrap_or(true));
    answer(serde_json::json!({ "success": true }))
}

fn op_def(name: &str) -> azul_core::json::Json {
    azul_core::json::Json::parse(
        &serde_json::json!({ "name": name, "summary": format!("the {name} test command") }).to_string(),
    )
    .expect("a valid op definition")
}

fn config() -> AppConfig {
    let mut config = AppConfig::create();
    let commands: [(&str, E2eCommandCallbackType); 2] = [("play", play), ("mute", mute)];
    for (name, cb) in commands {
        config.add_e2e_command(
            AzString::from(name.to_string()),
            op_def(name),
            E2eCommandCallback::create(cb).to_core(),
        );
    }
    config
}

fn run(name: &str, steps: serde_json::Value) -> E2eTestResult {
    let test: E2eTest = serde_json::from_value(serde_json::json!({
        "name": name,
        "config": { "continue_on_failure": true },
        "setup": { "window_width": 200, "window_height": 100, "dpi": 96 },
        "steps": steps,
    }))
    .expect("the scenario literal is a valid E2eTest");
    run_e2e_test_with_app(&test, RefAny::new(()), &config())
}

fn errors(result: &E2eTestResult) -> Vec<String> {
    result.steps.iter().filter_map(|s| s.error.clone()).collect()
}

fn assert_passes(result: &E2eTestResult) {
    assert!(
        result.status == "pass" && result.steps_failed == 0,
        "scenario '{}' failed: {:#?}",
        result.name,
        errors(result)
    );
}

/// What this host's route table says for `sound` in the runner (no OS switch read: the default
/// metrics), as the text `assert_sound`'s `route` takes.
fn host_route(sound: SystemSound) -> String {
    route(
        sound,
        SoundPlatform::current(),
        SoundConfig::default(),
        AudioMetrics::default(),
    )
    .label()
}

#[test]
fn a_request_of_a_callback_is_recorded_and_assert_sound_counts_it() {
    let result = run(
        "recorded",
        serde_json::json!([
            { "op": "assert_no_sound" },
            { "op": "play", "sound": "TabChange" },
            { "op": "assert_sound", "sound": "TabChange" },
            { "op": "assert_sound", "sound": "TabChange", "count": 1 },
            { "op": "assert_sound", "sound": "Error", "count": 0 },
            { "op": "assert_sound", "class": "input", "count": 1 },
            { "op": "assert_sound", "class": "event", "count": 0 },
            { "op": "assert_no_sound", "sound": "Error" },
        ]),
    );
    assert_passes(&result);
}

#[test]
fn one_tick_per_page_of_a_fling_is_five_records_for_five_requests() {
    // the TabChange of a pager that crosses five pages: every crossing is a request, and a
    // headless run records each one (the pacing belongs to a shell with a clock)
    // Whether a request PLAYS depends on the host (Android has a route, a desktop has none); the
    // number of requests and the route the host's table names do not.
    let result = run(
        "fling",
        serde_json::json!([
            { "op": "play", "sound": "TabChange", "times": 5 },
            { "op": "assert_sound", "sound": "TabChange", "count": 5 },
            { "op": "assert_sound", "sound": "TabChange", "count": 5,
              "route": host_route(SystemSound::TabChange) },
        ]),
    );
    assert_passes(&result);
}

#[test]
fn since_counts_only_the_requests_after_a_sequence_number() {
    let result = run(
        "since",
        serde_json::json!([
            { "op": "play", "sound": "Click", "times": 3 },
            { "op": "play", "sound": "Tick", "times": 2 },
            { "op": "assert_sound", "count": 5 },
            { "op": "assert_sound", "since": 3, "count": 2 },
            { "op": "assert_sound", "since": 3, "sound": "Tick", "count": 2 },
            { "op": "assert_sound", "since": 3, "sound": "Click", "count": 0 },
            { "op": "assert_sound", "since": 5, "count": 0 },
        ]),
    );
    assert_passes(&result);
}

#[test]
fn the_apps_mute_is_recorded_as_the_reason_a_request_did_not_play() {
    let result = run(
        "muted",
        serde_json::json!([
            { "op": "mute", "muted": true },
            { "op": "play", "sound": "Error", "times": 2 },
            { "op": "assert_sound", "sound": "Error", "count": 2, "suppressed_by": "app-muted" },
            { "op": "assert_sound", "sound": "Error", "count": 2, "played": false },
            { "op": "assert_sound", "sound": "Error", "count": 0, "played": true },
            { "op": "assert_no_sound", "played": true },
            { "op": "mute", "muted": false },
            { "op": "play", "sound": "Error" },
            { "op": "assert_sound", "sound": "Error", "count": 3 },
            { "op": "assert_sound", "sound": "Error", "count": 2, "suppressed_by": "app-muted" },
        ]),
    );
    assert_passes(&result);
}

#[test]
fn the_route_a_request_recorded_is_the_route_of_this_hosts_table() {
    let result = run(
        "route",
        serde_json::json!([
            { "op": "play", "sound": "Notification" },
            { "op": "assert_sound", "sound": "Notification", "route": host_route(SystemSound::Notification) },
            { "op": "assert_sound", "sound": "Notification", "count": 0, "route": "system:nowhere:NOPE" },
        ]),
    );
    assert_passes(&result);
}

#[test]
fn assert_no_sound_fails_naming_what_was_recorded() {
    let result = run(
        "no sound",
        serde_json::json!([
            { "op": "play", "sound": "Bell" },
            { "op": "assert_no_sound" },
            { "op": "assert_no_sound", "sound": "Bell" },
            { "op": "assert_no_sound", "sound": "Click" },
        ]),
    );
    let errors = errors(&result);
    assert_eq!(errors.len(), 2, "{errors:#?}");
    assert!(errors[0].contains("a recorded sound request matches"), "{errors:#?}");
    assert!(errors[1].contains("a recorded sound request matches"), "{errors:#?}");
}

#[test]
fn assert_sound_fails_when_nothing_matches_and_says_what_the_recording_holds() {
    let result = run(
        "nothing matches",
        serde_json::json!([
            { "op": "play", "sound": "Click" },
            { "op": "assert_sound", "sound": "Back" },
            { "op": "assert_sound", "sound": "Click", "count": 2 },
        ]),
    );
    let errors = errors(&result);
    assert_eq!(errors.len(), 2, "{errors:#?}");
    assert!(errors[0].contains("no recorded sound request"), "{errors:#?}");
    assert!(errors[1].contains("count differs"), "{errors:#?}");
}

#[test]
fn a_typo_in_a_filter_is_refused_by_name_and_does_not_pass_for_nothing() {
    let result = run(
        "typos",
        serde_json::json!([
            { "op": "assert_sound", "sound": "Thunk" },
            { "op": "assert_sound", "class": "loud" },
            { "op": "assert_sound", "sund": "Click" },
            { "op": "assert_no_sound", "count": 0 },
        ]),
    );
    let errors = errors(&result);
    assert_eq!(errors.len(), 4, "every step is refused: {errors:#?}");
    assert!(errors[0].contains("not a system sound") && errors[0].contains("TabChange"), "{errors:#?}");
    assert!(errors[1].contains("`input` or `event`"), "{errors:#?}");
    assert!(errors[2].contains("unknown parameter 'sund'"), "{errors:#?}");
    assert!(errors[3].contains("unknown parameter 'count'"), "{errors:#?}");
}
