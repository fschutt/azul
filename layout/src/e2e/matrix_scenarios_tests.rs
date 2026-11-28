//! Scenarios of QA's e2e matrix (`tests/e2e/*.json`, run on every platform) that found a bug of
//! the layout, run through the real op dispatch (`run_e2e_test`) on a headless window - so that
//! the bug is a red test here and not only a red cell of the matrix.
//!
//! - `layout23_proof.json`, "an inline list item shows its marker before its text on the same
//!   line": the two `display: inline list-item` items of a list are one line (a list 20 px high),
//!   not two (40 px).
//! - `css23_proof.json`, "a frame takes its focus-within style while a field inside it has the
//!   focus": Tab into the field grows the padding of the frame (`.frame:focus-within`) and the
//!   frame is 26 px high, a click outside takes it away again (14 px).

use super::{load_e2e_tests, run_e2e_test, E2eTest, E2eTestResult};

/// The scenario of `tests/e2e/<file>` whose name contains `name`.
fn scenario(file: &str, name: &str) -> E2eTest {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/e2e")
        .join(file);
    load_e2e_tests(&path)
        .unwrap_or_else(|e| panic!("the matrix file loads: {e}"))
        .into_iter()
        .find(|t| t.name.contains(name))
        .unwrap_or_else(|| panic!("{file} has no scenario named like `{name}`"))
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

fn assert_passes(file: &str, name: &str) {
    let mut test = scenario(file, name);
    // a red step does not end the run: every assertion is read
    test.config.continue_on_failure = true;
    let result = run_e2e_test(&test);
    assert!(
        result.status == "pass" && result.steps_failed == 0,
        "scenario '{}' failed:\n{}",
        result.name,
        failures(&result)
    );
}

#[test]
fn the_items_of_an_inline_list_are_one_line() {
    assert_passes("layout23_proof.json", "an inline list item shows its marker");
}

#[test]
fn a_frame_takes_its_focus_within_style_and_its_relayout_while_a_field_inside_has_the_focus() {
    assert_passes("css23_proof.json", "a frame takes its focus-within style");
}
