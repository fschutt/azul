//! A frame whose only change is a MOVE is not an idle frame, driven through the
//! real op dispatch (`run_e2e_test`) on a headless window.
//!
//! The layout pass behind a geometry override can move boxes by ONE delta:
//! `justify-content: space-between` on two flex items moves the second item and
//! nothing else, `flex-direction: column` moves the second item diagonally,
//! `align-items: center` moves both items down. The item diff then counts the
//! moved items as MOVED rather than damaged (`compute_display_list_damage_live`),
//! so such a frame has an empty damage list and a moved union, and the pixels
//! travel only in `cpurender::execute_translate_blit` - which a skipped frame
//! never reaches. The shells' presenter skips a frame only when no move is
//! pending (6f705cd6f); the runner's twin did not ask, skipped these frames,
//! reported `FrameDamage::None` and kept the old window on screen while a fresh
//! render differed in tens of thousands of pixels. Batch 3 of `gen-e2e` read that
//! as three engine bugs (lines 27, 38 and 40: `assert_changed` said `STALE SCREEN
//! (engine)`; the family is about 195 corpus lines). Both presenters now ask the
//! one `cpurender::FrameChanges::is_idle`.
//!
//! Every scenario here is a fixture of the review's probes (`p15`, `p8`, `p9`,
//! `p10`) or of the generated scenarios themselves, and asserts what the
//! generated scenarios assert of such a control: layout ran, paint damage exists
//! and a pixel differs, present damage exists, and the present damage covers
//! every pixel that changed.

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

/// `html` / `css` mounted and landed, a snapshot as `before`, then `property` of
/// `selector` overridden to `value`, a frame to land it, and the assertions the
/// generated scenarios make of such a control.
fn a_uniform_move(
    name: &str,
    html: &[&str],
    css: &[&str],
    selector: &str,
    property: &str,
    value: &str,
) -> E2eTestResult {
    run(
        name,
        vec![
            serde_json::json!({ "op": "mount", "html": html, "css": css }),
            serde_json::json!({ "op": "wait_frame" }),
            serde_json::json!({ "op": "wait", "ms": 100 }),
            serde_json::json!({ "op": "reset_frame_counters" }),
            serde_json::json!({ "op": "snapshot_frame", "as": "before" }),
            serde_json::json!({
                "op": "set_node_css_override", "selector": selector,
                "property": property, "value": value,
            }),
            serde_json::json!({ "op": "wait_frame" }),
            serde_json::json!({ "op": "wait", "ms": 100 }),
            // The override took: geometry ran layout.
            serde_json::json!({ "op": "assert_work_bounded", "min_layout_passes": 1 }),
            // PAINT damage exists (the vacated strips) and a pixel differs.
            serde_json::json!({ "op": "assert_changed", "vs": "before", "min_damage_rects": 1 }),
            // PRESENT damage exists (the moved block is on it) ...
            serde_json::json!({ "op": "assert_changed", "vs": "before", "which": "present" }),
            // ... and covers every pixel that changed: the vacated ones and the
            // ones the blit carried to their new place.
            serde_json::json!({
                "op": "assert_damage_covers_changes", "vs": "before", "which": "present",
            }),
        ],
    )
}

/// The stylesheet and markup of the review's probes `p8` / `p15`: two blue items
/// on a red flex container that fills the window.
const TWO_ITEMS_ON_RED_HTML: [&str; 6] = [
    "<div id=\"frame\">",
    "  <div id=\"flex\">",
    "    <div class=\"item\" id=\"a\"></div>",
    "    <div class=\"item\" id=\"b\"></div>",
    "  </div>",
    "</div>",
];
const TWO_ITEMS_ON_RED_CSS: [&str; 4] = [
    "html, body { margin: 0; width: 100%; height: 100%; }",
    "#frame { width: 400px; height: 300px; }",
    "#flex { display: flex; flex-direction: row; justify-content: flex-start; align-items: \
     stretch; width: 400px; height: 300px; background: red; }",
    ".item { width: 60px; background: blue; }",
];

/// Batch 3, line 27 (probe `p15`): `justify-content: space-between` on two flex
/// items. The first stays at x=0, the second moves from x=60 to x=340: ONE node
/// translated by a uniform delta, nothing else changed. The item diff has no
/// damage for it, so the old runner skipped the frame: `assert_changed` failed
/// with "STALE SCREEN (engine): nothing was repainted, but a fresh render differs
/// from snapshot 'before' in 36000 px", and so did the coverage check.
#[test]
fn b3_27_space_between_moves_one_item_and_the_frame_is_not_skipped() {
    assert_passes(&a_uniform_move(
        "b3_27_space_between",
        &TWO_ITEMS_ON_RED_HTML,
        &TWO_ITEMS_ON_RED_CSS,
        "#flex",
        "justify-content",
        "space-between",
    ));
}

/// Probe `p8`: `justify-content: flex-end` moves BOTH items by the same delta
/// (x=0 -> 280 and x=60 -> 340): every moved node translated uniformly.
#[test]
fn b3_p8_flex_end_moves_both_items_and_the_frame_is_not_skipped() {
    assert_passes(&a_uniform_move(
        "b3_p8_flex_end",
        &TWO_ITEMS_ON_RED_HTML,
        &TWO_ITEMS_ON_RED_CSS,
        "#flex",
        "justify-content",
        "flex-end",
    ));
}

/// Batch 3, line 38 (probe `p9` is the same page on plain divs): `flex-direction:
/// column` on a row of a blue and a red item, both 80x60 and fixed. The first
/// stays, the second moves by (-80, +60) without resizing. The old runner
/// reported "STALE SCREEN (engine) ... in 9600 px".
#[test]
fn b3_38_flex_direction_column_moves_one_item_and_the_frame_is_not_skipped() {
    assert_passes(&a_uniform_move(
        "b3_38_flex_direction_column",
        &[
            "<div id=\"row\">",
            "  <img id=\"pic\" class=\"item\">",
            "  <div id=\"sib\" class=\"item2\"></div>",
            "</div>",
        ],
        &[
            "#row { display: flex; flex-direction: row; width: 400px; height: 300px; \
             background: white; }",
            ".item { width: 80px; height: 60px; background-color: blue; }",
            ".item2 { width: 80px; height: 60px; background-color: red; }",
        ],
        "#row",
        "flex-direction",
        "column",
    ));
}

/// Batch 3, line 40: `justify-content: space-between` on a row holding an
/// `<img>` and a red item, 60x60 each, on a grey row 100px high. The old runner
/// reported "STALE SCREEN (engine) ... in 7200 px" for the present damage.
#[test]
fn b3_40_space_between_in_an_image_row_moves_one_item_and_the_frame_is_not_skipped() {
    assert_passes(&a_uniform_move(
        "b3_40_space_between_image_row",
        &[
            "<div id=\"row\">",
            "  <img id=\"pic\" class=\"item\">",
            "  <div id=\"b\" class=\"item\"></div>",
            "</div>",
        ],
        &[
            "html, body { margin: 0; background: white; }",
            "#row { display: flex; flex-direction: row; justify-content: flex-start; \
             width: 400px; height: 100px; background: #dddddd; }",
            ".item { width: 60px; height: 60px; background: blue; }",
            "#b { background: red; }",
        ],
        "#row",
        "justify-content",
        "space-between",
    ));
}

/// Probe `p10`: `align-items: center` on two same-height items moves BOTH down by
/// the same delta (y=0 -> 120).
#[test]
fn b3_p10_align_items_center_moves_both_items_and_the_frame_is_not_skipped() {
    assert_passes(&a_uniform_move(
        "b3_p10_align_items_center",
        &[
            "<div id=\"row\">",
            "  <div id=\"a\" class=\"item\"></div>",
            "  <div id=\"b\" class=\"item\"></div>",
            "</div>",
        ],
        &[
            "#row { display: flex; flex-direction: row; align-items: flex-start; \
             width: 400px; height: 300px; background: white; }",
            ".item { width: 80px; height: 60px; background-color: blue; }",
            "#b { background-color: red; }",
        ],
        "#row",
        "align-items",
        "center",
    ));
}

/// The same two items on a red flex container, mounted with `justify` as its
/// `justify-content`.
fn two_items_mounted_with(justify: &str) -> serde_json::Value {
    serde_json::json!({
        "op": "mount",
        "html": TWO_ITEMS_ON_RED_HTML,
        "css": [
            "html, body { margin: 0; width: 100%; height: 100%; }",
            "#frame { width: 400px; height: 300px; }",
            format!(
                "#flex {{ display: flex; flex-direction: row; justify-content: {justify}; \
                 align-items: stretch; width: 400px; height: 300px; background: red; }}"
            ),
            ".item { width: 60px; background: blue; }",
        ],
    })
}

/// E2E-R12, the contrast of the review's `jc_mount.json`: the same move arriving
/// with a SECOND `mount` of the identical markup and a changed `justify-content`
/// (not an override) showed the same under-paint - "so this is not the runner's
/// double pass". The new document keeps the structure, the layout moves the
/// second item by one delta, and the frame has to be painted.
#[test]
fn r12_a_second_mount_that_only_changes_the_justify_content_paints_the_move() {
    assert_passes(&run(
        "r12_second_mount",
        vec![
            two_items_mounted_with("flex-start"),
            serde_json::json!({ "op": "wait_frame" }),
            serde_json::json!({ "op": "wait", "ms": 100 }),
            serde_json::json!({ "op": "reset_frame_counters" }),
            serde_json::json!({ "op": "snapshot_frame", "as": "before" }),
            two_items_mounted_with("space-between"),
            serde_json::json!({ "op": "wait_frame" }),
            serde_json::json!({ "op": "wait", "ms": 100 }),
            serde_json::json!({ "op": "assert_changed", "vs": "before", "min_damage_rects": 1 }),
            serde_json::json!({ "op": "assert_changed", "vs": "before", "which": "present" }),
            serde_json::json!({
                "op": "assert_damage_covers_changes", "vs": "before", "which": "present",
            }),
        ],
    ));
}
