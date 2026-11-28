//! The op `list_popups` through the real dispatcher: the same question on every host, answered from
//! the engine's own record of the open popups ([`crate::window::LayoutWindow::open_popups`], tested
//! with the layout on a phone and on a desktop in `a_window_with_one_surface_shows_its_popups_inside_itself`).
//!
//! The in-crate runner is no shell and shows no popup; what it can prove is the op's shape and that a
//! page without a popup lists none.

use azul_core::{dom::Dom, styled_dom::StyledDom};

use super::{runner::run_e2e_test_with_dom, E2eTest, E2eTestResult};

fn run(steps: serde_json::Value) -> E2eTestResult {
    let mut dom = Dom::create_body().with_child(Dom::create_div_with_text("A page"));
    let (css, _) = azul_css::parser2::new_from_str("* { margin: 0; padding: 0; }");
    let test: E2eTest = serde_json::from_value(serde_json::json!({
        "name": "list_popups",
        "config": { "continue_on_failure": true },
        "setup": { "window_width": 411, "window_height": 400, "dpi": 96 },
        "steps": steps,
    }))
    .expect("the scenario literal is a valid E2eTest");
    run_e2e_test_with_dom(&test, Some(StyledDom::create(&mut dom, css)))
}

#[test]
fn a_page_without_a_popup_lists_none() {
    let result = run(serde_json::json!([
        { "op": "wait_frame" },
        { "op": "list_popups" },
        { "op": "assert_response", "contains": "\"popup_count\":0" },
        { "op": "assert_response", "contains": "\"popups\":[]" },
    ]));
    let reds: Vec<String> = result
        .steps
        .iter()
        .filter(|s| s.status != "pass")
        .map(|s| format!("  step {} `{}`: {}", s.step_index, s.op, s.error.clone().unwrap_or_default()))
        .collect();
    assert!(
        result.status == "pass" && result.steps_failed == 0,
        "scenario '{}' failed:\n{}",
        result.name,
        reds.join("\n")
    );
}
