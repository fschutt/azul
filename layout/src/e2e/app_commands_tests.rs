//! An application's own E2E ops (`AppConfig::add_e2e_command`), driven end
//! to end through the real dispatcher on a headless window: a step whose op
//! no engine op takes reaches the app's command with its fields as the
//! arguments, `"success": false` fails the step, `"pending": true` runs it
//! again, and `assert_response` reads one value of the answer by its path.

use azul_core::{
    events::{CustomE2eOpResult, E2eCommand},
    refany::RefAny,
    resources::AppConfig,
};
use azul_css::AzString;
use azul_layout::callbacks::{CallbackInfo, E2eCommandCallback, E2eCommandCallbackType};

use super::{
    full::{parse_debug_event, DebugEvent},
    runner::run_e2e_test_with_app,
    E2eTest, E2eTestResult,
};

/// The app data the commands below count their calls in.
struct Counter {
    calls: u64,
}

fn answer(json: serde_json::Value) -> CustomE2eOpResult {
    CustomE2eOpResult {
        handled: true,
        json: AzString::from(json.to_string()),
    }
}

fn args_of(args: &AzString) -> serde_json::Value {
    serde_json::from_str(args.as_str()).unwrap_or(serde_json::Value::Null)
}

/// Answers its own name and arguments, and counts the call in the app data.
extern "C" fn echo(mut data: RefAny, _info: CallbackInfo, name: AzString, args: AzString) -> CustomE2eOpResult {
    let calls = match data.downcast_mut::<Counter>() {
        Some(mut counter) => {
            counter.calls += 1;
            counter.calls
        }
        None => 0,
    };
    answer(serde_json::json!({
        "success": true,
        "name": name.as_str(),
        "got": args_of(&args),
        "calls": calls,
        "files": { "a.txt": { "size": 3 } },
        "list": [1, 2],
    }))
}

/// Always fails, saying why.
extern "C" fn refuse(_data: RefAny, _info: CallbackInfo, _name: AzString, _args: AzString) -> CustomE2eOpResult {
    answer(serde_json::json!({ "success": false, "error": "no such device: z" }))
}

/// Pending until its fourth call (`_attempt` 3), then done.
extern "C" fn slow(_data: RefAny, _info: CallbackInfo, _name: AzString, args: AzString) -> CustomE2eOpResult {
    let attempt = args_of(&args).get("_attempt").and_then(serde_json::Value::as_u64);
    match attempt {
        Some(n) if n >= 3 => answer(serde_json::json!({ "success": true, "attempts": n + 1 })),
        Some(_) => answer(serde_json::json!({ "success": true, "pending": true })),
        None => answer(serde_json::json!({ "success": false, "error": "no _attempt" })),
    }
}

/// Never done.
extern "C" fn forever(_data: RefAny, _info: CallbackInfo, _name: AzString, _args: AzString) -> CustomE2eOpResult {
    answer(serde_json::json!({ "success": true, "pending": true }))
}

/// The app's catch-all handler (`AppConfig::custom_e2e_op`).
extern "C" fn catch_all(_data: RefAny, name: AzString, _args: AzString) -> CustomE2eOpResult {
    if name.as_str() == "anything_else" {
        answer(serde_json::json!({ "success": true, "via": "catch-all" }))
    } else {
        CustomE2eOpResult::default()
    }
}

fn op_def(name: &str) -> azul_core::json::Json {
    azul_core::json::Json::parse(
        &serde_json::json!({ "name": name, "summary": format!("the {name} test command") }).to_string(),
    )
    .expect("a valid op definition")
}

fn config() -> AppConfig {
    let mut config = AppConfig::create();
    let commands: [(&str, E2eCommandCallbackType); 4] =
        [("echo", echo), ("refuse", refuse), ("slow", slow), ("forever", forever)];
    for (name, cb) in commands {
        config.add_e2e_command(
            AzString::from(name.to_string()),
            op_def(name),
            E2eCommandCallback::create(cb).to_core(),
        );
    }
    config.custom_e2e_op = azul_core::events::CustomE2eOpCallback::from(
        catch_all as azul_core::events::CustomE2eOpCallbackType,
    );
    config
}

fn run(name: &str, continue_on_failure: bool, steps: serde_json::Value) -> E2eTestResult {
    let test: E2eTest = serde_json::from_value(serde_json::json!({
        "name": name,
        "config": { "continue_on_failure": continue_on_failure },
        "setup": { "window_width": 200, "window_height": 100, "dpi": 96 },
        "steps": steps,
    }))
    .expect("the scenario literal is a valid E2eTest");
    run_e2e_test_with_app(&test, RefAny::new(Counter { calls: 0 }), &config())
}

fn failures(result: &E2eTestResult) -> String {
    result
        .steps
        .iter()
        .filter(|s| s.status != "pass")
        .map(|s| format!("  step {} `{}`: {}", s.step_index, s.op, s.error.clone().unwrap_or_default()))
        .collect::<Vec<_>>()
        .join("\n")
}

fn assert_passes(result: &E2eTestResult) {
    assert!(
        result.status == "pass" && result.steps_failed == 0,
        "scenario '{}' failed:\n{}",
        result.name,
        failures(result)
    );
}

fn step_error(result: &E2eTestResult, index: usize) -> String {
    result
        .steps
        .iter()
        .find(|s| s.step_index == index)
        .and_then(|s| s.error.clone())
        .unwrap_or_default()
}

#[test]
fn an_op_no_engine_op_has_reaches_the_apps_command_with_the_steps_fields_as_its_arguments() {
    let result = run(
        "echo",
        false,
        serde_json::json!([
            { "op": "echo", "device": "a", "n": 3 },
            { "op": "assert_response", "path": "result.name", "equals": "echo" },
            { "op": "assert_response", "path": "result.got.device", "equals": "a" },
            { "op": "assert_response", "path": "result.got.n", "equals": 3 },
            { "op": "assert_response", "path": "result.got._attempt", "equals": 0 },
            // The long spelling reaches the same command.
            { "op": "custom_op", "name": "echo", "args": { "device": "b" } },
            { "op": "assert_response", "path": "result.got.device", "equals": "b" },
        ]),
    );
    assert_passes(&result);
}

#[test]
fn a_command_receives_the_apps_own_data() {
    let result = run(
        "the app's data",
        false,
        serde_json::json!([
            { "op": "echo" },
            { "op": "echo" },
            { "op": "assert_response", "path": "result.calls", "equals": 2 },
        ]),
    );
    assert_passes(&result);
}

#[test]
fn a_command_answering_success_false_fails_its_step_with_the_commands_own_error() {
    let result = run("refuse", false, serde_json::json!([{ "op": "refuse" }, { "op": "echo" }]));
    assert_eq!(result.status, "fail", "a refused command must fail the scenario");
    let error = step_error(&result, 0);
    assert!(error.contains("no such device: z"), "the step says the command's error: {error}");
    assert_eq!(result.steps.len(), 1, "the scenario stops at the failed step");
}

#[test]
fn a_pending_answer_runs_the_step_again_until_the_command_is_done() {
    let result = run(
        "slow",
        false,
        serde_json::json!([
            { "op": "slow" },
            { "op": "assert_response", "path": "result.attempts", "equals": 4 },
        ]),
    );
    assert_passes(&result);
}

#[test]
fn a_command_that_stays_pending_fails_its_step_once_the_steps_timeout_passed() {
    let result = run("forever", false, serde_json::json!([{ "op": "forever", "timeout_ms": 50 }]));
    assert_eq!(result.status, "fail");
    let error = step_error(&result, 0);
    assert!(error.contains("still pending after 50 ms"), "the step says it timed out: {error}");
}

#[test]
fn an_op_neither_the_engine_nor_the_app_answers_fails() {
    let result = run("unknown", false, serde_json::json!([{ "op": "no_such_op_anywhere" }]));
    assert_eq!(result.status, "fail");
    let error = step_error(&result, 0);
    assert!(error.contains("neither an engine op nor one the application answers"), "{error}");
}

#[test]
fn the_apps_catch_all_handler_from_its_config_answers_what_no_command_takes() {
    let result = run(
        "catch-all",
        false,
        serde_json::json!([
            { "op": "anything_else" },
            { "op": "assert_response", "path": "result.via", "equals": "catch-all" },
        ]),
    );
    assert_passes(&result);
}

#[test]
fn only_an_unknown_op_name_is_handed_to_the_app_a_known_op_with_wrong_fields_stays_an_error() {
    match parse_debug_event(serde_json::json!({ "op": "drive_sync", "device": "a" })) {
        Ok(DebugEvent::CustomOp { name, args }) => {
            assert_eq!(name, "drive_sync");
            assert_eq!(args, serde_json::json!({ "device": "a" }));
        }
        other => panic!("an unknown op becomes CustomOp, got {other:?}"),
    }
    assert!(
        parse_debug_event(serde_json::json!({ "op": "mouse_move", "x": "left" })).is_err(),
        "mouse_move with a string x is an engine op's error, not an app op"
    );
}

#[test]
fn assert_response_reads_a_path_with_quoted_keys_and_indices_and_compares_numbers_by_value() {
    let result = run(
        "paths",
        true,
        serde_json::json!([
            { "op": "echo" },
            { "op": "assert_response", "path": "result.files[\"a.txt\"].size", "equals": 3.0 },
            { "op": "assert_response", "path": "result.list", "len": 2 },
            { "op": "assert_response", "path": "result.list[1]", "at_least": 2, "at_most": 2 },
            { "op": "assert_response", "path": "result.name", "not_equals": "other" },
            // Step 5 must fail: the path is not in the answer.
            { "op": "assert_response", "path": "result.missing", "equals": 1 },
            // Step 6 must fail: the value is out of range.
            { "op": "assert_response", "path": "result.list[0]", "at_least": 5 },
        ]),
    );
    let failed: Vec<usize> = result
        .steps
        .iter()
        .filter(|s| s.status != "pass")
        .map(|s| s.step_index)
        .collect();
    assert_eq!(failed, vec![5, 6], "exactly the two wrong assertions fail:\n{}", failures(&result));
    assert!(step_error(&result, 5).contains("has no 'result.missing'"));
}

#[test]
fn list_app_ops_answers_every_command_the_app_added() {
    let result = run(
        "list",
        false,
        serde_json::json!([
            { "op": "list_app_ops" },
            { "op": "assert_response", "path": "commands", "len": 4 },
            { "op": "assert_response", "path": "commands[0].name", "equals": "echo" },
            { "op": "assert_response", "path": "commands[3].summary", "equals": "the forever test command" },
        ]),
    );
    assert_passes(&result);
}

#[test]
fn adding_a_command_under_a_name_it_has_replaces_the_earlier_one() {
    let mut config = AppConfig::create();
    for cb in [echo as E2eCommandCallbackType, refuse] {
        config.add_e2e_command(
            AzString::from("same".to_string()),
            op_def("same"),
            E2eCommandCallback::create(cb).to_core(),
        );
    }
    let commands: &[E2eCommand] = config.e2e_commands.as_ref();
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].callback.cb, refuse as E2eCommandCallbackType as usize);
}
