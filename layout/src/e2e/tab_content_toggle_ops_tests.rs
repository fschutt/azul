//! The window's tab content flag through the real dispatcher: the ops `toggle_tab_content` and
//! `get_tab_content`, and the click on the strip's chevron, flip and read the same flag.

use azul_core::{dom::Dom, styled_dom::StyledDom};

use super::{runner::run_e2e_test_with_dom, E2eTest, E2eTestResult};
use crate::widgets::tabs::{TabHeader, TabStyle};

fn run(steps: serde_json::Value, style: TabStyle) -> E2eTestResult {
    let labels = azul_css::StringVec::from_vec((0..4).map(|i| azul_css::AzString::from(format!("Tab {i}"))).collect());
    let mut dom = Dom::create_body().with_child(
        TabHeader::create(labels).with_style(style).with_content_toggle(true).dom(),
    );
    let (css, _) = azul_css::parser2::new_from_str("* { margin: 0; padding: 0; }");
    let test: E2eTest = serde_json::from_value(serde_json::json!({
        "name": "tab_content_flag",
        "config": { "continue_on_failure": true },
        "setup": { "window_width": 411, "window_height": 400, "dpi": 96 },
        "steps": steps,
    }))
    .expect("the scenario literal is a valid E2eTest");
    run_e2e_test_with_dom(&test, Some(StyledDom::create(&mut dom, css)))
}

fn assert_passes(result: &E2eTestResult) {
    let reds: Vec<String> = result
        .steps
        .iter()
        .filter(|s| s.status != "pass")
        .map(|s| format!("  step {} `{}`: {}", s.step_index, s.op, s.error.clone().unwrap_or_default()))
        .collect();
    assert!(result.status == "pass" && result.steps_failed == 0, "scenario '{}' failed:\n{}", result.name, reds.join("\n"));
}

#[test]
fn the_ops_flip_set_and_read_the_windows_tab_content_flag() {
    assert_passes(&run(
        serde_json::json!([
            { "op": "wait_frame" },
            { "op": "get_tab_content" },
            { "op": "assert_response", "contains": "\"hidden\":false" },
            { "op": "toggle_tab_content" },
            { "op": "wait_frame" },
            { "op": "get_tab_content" },
            { "op": "assert_response", "contains": "\"hidden\":true" },
            { "op": "toggle_tab_content" },
            { "op": "wait_frame" },
            { "op": "get_tab_content" },
            { "op": "assert_response", "contains": "\"hidden\":false" },
            { "op": "toggle_tab_content", "hidden": true },
            { "op": "wait_frame" },
            { "op": "toggle_tab_content", "hidden": true },
            { "op": "wait_frame" },
            { "op": "get_tab_content" },
            { "op": "assert_response", "contains": "\"hidden\":true" },
        ]),
        TabStyle::Pills,
    ));
}

#[test]
fn a_click_on_the_chevron_folds_the_content_and_the_next_one_shows_it_again() {
    for style in [TabStyle::Rectangular, TabStyle::Pills, TabStyle::TouchPills] {
        assert_passes(&run(
            serde_json::json!([
                { "op": "wait_frame" },
                { "op": "click", "selector": ".__azul-tab-content-toggle" },
                { "op": "wait_frame" },
                { "op": "wait_frame" },
                { "op": "get_tab_content" },
                { "op": "assert_response", "contains": "\"hidden\":true" },
                { "op": "click", "selector": ".__azul-tab-content-toggle" },
                { "op": "wait_frame" },
                { "op": "wait_frame" },
                { "op": "get_tab_content" },
                { "op": "assert_response", "contains": "\"hidden\":false" },
            ]),
            style,
        ));
    }
}
