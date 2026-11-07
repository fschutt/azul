//! A box fades its overlay scrollbar by the `-azul-scrollbar-fade-delay` /
//! `-azul-scrollbar-fade-duration` it declares, driven through the real op
//! dispatch (`run_e2e_test`) on a headless window (E2E-R9).
//!
//! Both properties were parsed and never read: every bar fell by the window's
//! constants (500 ms visible, 200 ms fading). `gen-e2e` batch 1's scenario 11
//! declared a 200/100 ms fade and its idle window still caught the track
//! falling; with 3000 ms or 50 ms the failing frame was the same one.
//!
//! The scroll is programmatic (`scroll_node_by`), so the activity stamp the fade
//! counts from is the op's own, and the clock moves only by `tick_ms`: 250 ms
//! steps never land inside the default 200 ms fade of a bar whose last activity
//! was at the op. The window-level cases (zeros, one property alone, the exact
//! opacities) are in `layout/tests/a_scrollbar_fades_by_the_fade_css_of_its_box.rs`.

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
/// mounted, landed, scrolled once by an op and settled by one 250 ms tick -
/// the bar is on screen, and `visible` is its frame.
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
        serde_json::json!({
            "op": "scroll_node_by", "selector": "#scroller", "delta_x": 0, "delta_y": 60,
        }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "tick_ms", "ms": 250 }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "reset_frame_counters" }),
        serde_json::json!({ "op": "snapshot_frame", "as": "visible" }),
    ]
}

/// Three more 250 ms ticks, each followed by its frame: 1000 ms after the scroll.
fn a_second_later() -> Vec<serde_json::Value> {
    (0..3)
        .flat_map(|_| {
            [
                serde_json::json!({ "op": "tick_ms", "ms": 250 }),
                serde_json::json!({ "op": "wait_frame" }),
            ]
        })
        .collect()
}

/// The control: a bar that declares nothing is gone a second after the scroll
/// (500 ms hold + 200 ms fade), so the frame differs from the one with the bar -
/// the fixture CAN see a fade, and the test below means something.
#[test]
fn e2e_r9_a_bar_without_fade_css_is_gone_a_second_after_the_scroll() {
    let mut steps = scrolled_scroller("");
    steps.extend(a_second_later());
    steps.push(
        serde_json::json!({ "op": "assert_changed", "vs": "visible", "min_damage_rects": 1 }),
    );
    assert_passes(&run("e2e_r9_control", steps));
}

/// E2E-R9: a bar that declares a 3000 ms delay is still there a second after the
/// scroll - the frame is the one it had. The window's fade took it away at
/// 700 ms whatever the box said.
#[test]
fn e2e_r9_a_bar_with_a_long_declared_delay_is_still_there_a_second_after() {
    let mut steps = scrolled_scroller(
        "-azul-scrollbar-fade-delay: 3000ms; -azul-scrollbar-fade-duration: 300ms;",
    );
    steps.extend(a_second_later());
    steps.push(serde_json::json!({ "op": "assert_idle_stable", "vs": "visible" }));
    assert_passes(&run("e2e_r9_long_delay", steps));
}
