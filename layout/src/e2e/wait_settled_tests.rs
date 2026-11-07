//! `wait_settled` in a scripted run waits: it moves the engine's clock until the
//! window is at rest or `timeout_ms` of ENGINE time is spent, driven through the
//! real op dispatch (`run_e2e_test`) on a headless window (E2E-M3).
//!
//! In a scripted run the engine clock moves only when the scenario moves it, and
//! the op used to answer at once: "settled" 50 ms after a wheel scroll with the
//! scrollbar track still to fall some 750 ms later (`window_still_moving` knew a
//! bar MID-fade, not the hold before it), or an error for anything in flight. A
//! scenario could neither wait for a fade nor trust the answer, and the
//! generator prompt told it `wait_settled` "does not help in this runner".

use super::{run_e2e_test, E2eTest, E2eTestResult};

/// Run `steps` as one headless scenario on a 400x300 window. A red step does not
/// end the run, so every assertion is read.
fn run(name: &str, steps: Vec<serde_json::Value>) -> E2eTestResult {
    let test: E2eTest = serde_json::from_value(serde_json::json!({
        "name": name,
        "config": { "continue_on_failure": true },
        "setup": { "window_width": 400, "window_height": 300, "dpi": 96 },
        "steps": steps,
    }))
    .expect("the scenario literal is a valid E2eTest");
    run_e2e_test(&test)
}

/// Every red step, one line each - what a failing run prints.
fn failures(result: &E2eTestResult) -> String {
    result
        .steps
        .iter()
        .filter(|s| s.status != "pass")
        .map(|s| {
            format!(
                "  step {} `{}`: {}",
                s.step_index,
                s.op,
                s.error.clone().unwrap_or_default()
            )
        })
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

/// A 300x200 scroller of 40 rows (`fade_css` = declarations on its rule),
/// mounted, landed and scrolled once by an op; `visible` is the frame with its
/// bar on screen.
fn scrolled_scroller(fade_css: &str) -> Vec<serde_json::Value> {
    let rows: String = (1..=40)
        .map(|i| format!("<div class=\"row\">Row {i}</div>"))
        .collect();
    vec![
        serde_json::json!({
            "op": "mount",
            "html": [format!("<div id=\"scroller\">{rows}</div>")],
            "css": [
                "html, body { margin: 0; }",
                format!(
                    "#scroller {{ width: 300px; height: 200px; overflow-y: scroll; \
                     background-color: white; font-family: \"Azul Mock Mono\"; {fade_css} }}"
                ),
                ".row { height: 30px; font-size: 14px; color: black; }".to_string(),
            ],
        }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "wait", "ms": 100 }),
        serde_json::json!({ "op": "reset_frame_counters" }),
        serde_json::json!({
            "op": "scroll_node_by", "selector": "#scroller", "delta_x": 0, "delta_y": 60,
        }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "snapshot_frame", "as": "visible" }),
    ]
}

/// E2E-M3: the bar is visible right after the scroll and `wait_settled` lets it
/// fall - the frame after it is the one without the bar, and every state machine
/// has noticed. (Before, the op answered at once: the bar was in its hold,
/// `scrollbar_fade_active` was false, and the frame was still the one with it.)
#[test]
fn e2e_m3_wait_settled_waits_out_a_scrollbar_fade() {
    let mut steps = scrolled_scroller("");
    steps.extend([
        serde_json::json!({ "op": "wait_settled" }),
        serde_json::json!({ "op": "assert_changed", "vs": "visible", "min_damage_rects": 1 }),
        serde_json::json!({ "op": "assert_state_machines_idle", "damage": false }),
    ]);
    assert_passes(&run("e2e_m3_waits_out_a_fade", steps));
}

/// E2E-M3: what does not come to rest by the deadline is an error that names it.
/// A bar that holds for 10 s is still to fall after 500 ms of engine time.
#[test]
fn e2e_m3_wait_settled_gives_up_at_its_timeout_and_names_what_still_moves() {
    let mut steps = scrolled_scroller(
        "-azul-scrollbar-fade-delay: 10000ms; -azul-scrollbar-fade-duration: 300ms;",
    );
    steps.push(serde_json::json!({ "op": "wait_settled", "timeout_ms": 500 }));
    let result = run("e2e_m3_gives_up", steps);
    let step = result
        .steps
        .iter()
        .find(|s| s.op == "wait_settled")
        .expect("the wait_settled step ran");
    assert_eq!(
        step.status, "fail",
        "a bar that holds for 10 s is not at rest in 500 ms"
    );
    let error = step.error.clone().unwrap_or_default();
    assert!(
        error.contains("still moving after 500 ms of engine time")
            && error.contains("a scrollbar waiting to fade out"),
        "the error names the deadline and what moved: {error}"
    );
}

/// E2E-M3: a window at rest is answered at once, with no engine time spent.
#[test]
fn e2e_m3_wait_settled_answers_a_window_at_rest_without_spending_time() {
    let result = run(
        "e2e_m3_at_rest",
        vec![
            serde_json::json!({
                "op": "mount",
                "html": ["<div id=\"box\"></div>"],
                "css": ["#box { width: 50px; height: 50px; background-color: red; }"],
            }),
            serde_json::json!({ "op": "wait_frame" }),
            serde_json::json!({ "op": "wait", "ms": 100 }),
            serde_json::json!({ "op": "wait_settled" }),
        ],
    );
    assert_passes(&result);
    let step = result
        .steps
        .iter()
        .find(|s| s.op == "wait_settled")
        .expect("the wait_settled step ran");
    assert!(
        step.logs
            .iter()
            .any(|l| l.contains("settled after 0 ms of engine time")),
        "no clock was moved for a window at rest: {:?}",
        step.logs
    );
}

/// E2E-M3: a parameter the op does not know is refused by name (a typo'd bound
/// used to be dropped by the parser, and the step waited for the default).
#[test]
fn e2e_m3_wait_settled_refuses_an_unknown_parameter_by_name() {
    let result = run(
        "e2e_m3_typo",
        vec![
            serde_json::json!({
                "op": "mount",
                "html": ["<div id=\"box\"></div>"],
                "css": ["#box { width: 50px; height: 50px; background-color: red; }"],
            }),
            serde_json::json!({ "op": "wait_frame" }),
            serde_json::json!({ "op": "wait_settled", "timeout": 500 }),
        ],
    );
    let step = result
        .steps
        .iter()
        .find(|s| s.op == "wait_settled")
        .expect("the wait_settled step ran");
    assert_eq!(step.status, "fail");
    let error = step.error.clone().unwrap_or_default();
    assert!(
        error.contains("wait_settled: unknown parameter 'timeout'") && error.contains("timeout_ms"),
        "the refusal names the op, the key and the known one: {error}"
    );
}
