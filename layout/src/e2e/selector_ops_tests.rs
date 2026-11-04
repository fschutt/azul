//! The DOM-mutation ops and `mouse_move` name their target by CSS `selector`,
//! driven end to end through the REAL op dispatch (`process_debug_event` + the
//! scenario runner) on a headless window.
//!
//! A numeric `node_id` counts EVERY node of the mounted document - `<html>` is
//! 0, `<body>` is 1, and each newline or indent between two tags is a
//! whitespace text node with an id of its own - so a scenario written by hand
//! (or by a model) lands on the wrong node as soon as its markup is indented
//! (44 of the 50 scenarios of the first `gen-e2e` batch addressed `<html>`,
//! `<body>` or a whitespace run instead of the box their line was about). A
//! selector names the node the scenario means, and the op refuses LOUDLY when
//! it names none or several: an op that quietly took the first match, or did
//! nothing, would let a scenario go green on a node nobody meant.
//!
//! `node_id` keeps working (the scenarios under `e2e/` use it); exactly one of
//! the two is given.

use super::{run_e2e_test, E2eTest, E2eTestResult};

/// The document every test mounts. It starts with `<body>` and has no
/// whitespace between tags, so the node ids are countable:
///
/// | id | node |
/// |----|------|
/// | 0 | `<html>` |
/// | 1 | `<body>` |
/// | 2 | `#root` |
/// | 3 / 4 | `#label` / its text `hello` |
/// | 5 | `#a.box` |
/// | 6 | `#b.box` |
/// | 7 | `#pic` (an `<img>`) |
/// | 8 | `#mixed` (`one<b>x</b>two`: two text children) |
/// | 9 / 10 / 11 / 12 | `one` / `<b>` / `x` / `two` |
/// | 13 | `#hidden` (`display:none`, no box) |
const DOM: &str = "<body><div id=\"root\"><p id=\"label\">hello</p><div id=\"a\" \
                   class=\"box\"></div><div id=\"b\" class=\"box\"></div><img id=\"pic\" \
                   /><p id=\"mixed\">one<b>x</b>two</p><div id=\"hidden\"></div></div></body>";

fn css() -> Vec<&'static str> {
    vec![
        "html, body { margin: 0; padding: 0; }",
        "#root { display: flex; flex-direction: row; }",
        ".box { width: 60px; height: 40px; background: #0000ff; }",
        "#a:hover { background: #ff0000; }",
        "#pic { width: 20px; height: 20px; }",
        "#hidden { display: none; }",
    ]
}

/// Run `steps` as one headless scenario on a 400x300 window.
fn run(name: &str, continue_on_failure: bool, steps: Vec<serde_json::Value>) -> E2eTestResult {
    let test: E2eTest = serde_json::from_value(serde_json::json!({
        "name": name,
        "config": { "continue_on_failure": continue_on_failure },
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

/// The mount, and a frame so the document exists for the steps after it.
fn mounted() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({ "op": "mount", "html": [DOM], "css": css() }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "assert_exists", "selector": "#root" }),
    ]
}

/// `base` with the fields of `extra` added.
fn with(mut base: serde_json::Value, extra: &serde_json::Value) -> serde_json::Value {
    let fields = base.as_object_mut().expect("a step is an object");
    for (k, v) in extra.as_object().expect("the target is an object") {
        fields.insert(k.clone(), v.clone());
    }
    base
}

/// The five DOM-mutation ops that take `node_id` / `selector`, each with the
/// payload it needs, aimed at `target` (`{"selector": ..}` or `{"node_id": ..}`).
fn mutation_ops(target: &serde_json::Value) -> Vec<serde_json::Value> {
    [
        serde_json::json!({ "op": "set_node_css_override", "property": "width", "value": "100px" }),
        serde_json::json!({ "op": "set_node_classes", "classes": ["x"] }),
        serde_json::json!({ "op": "set_node_text", "text": "t" }),
        serde_json::json!({ "op": "set_node_image", "width": 4, "height": 4, "color": "#ff0000" }),
        serde_json::json!({ "op": "delete_node" }),
    ]
    .iter()
    .map(|op| with(op.clone(), target))
    .collect()
}

/// Run `ops` after the mount with `continue_on_failure`, and give back each
/// op's refusal (the error of its step), in order. Panics when one passed.
fn refusals(name: &str, ops: Vec<serde_json::Value>) -> Vec<String> {
    let first = mounted().len();
    let mut steps = mounted();
    steps.extend(ops.iter().cloned());
    let result = run(name, true, steps);
    ops.iter()
        .enumerate()
        .map(|(i, op)| {
            let step = result
                .steps
                .iter()
                .find(|s| s.step_index == first + i)
                .unwrap_or_else(|| {
                    panic!(
                        "step {} ({op}) never ran:\n{}",
                        first + i,
                        failures(&result)
                    )
                });
            assert_eq!(
                step.status,
                "fail",
                "{op} must be REFUSED, but it passed:\n{}",
                failures(&result)
            );
            step.error.clone().unwrap_or_default()
        })
        .collect()
}

fn props(selector: &str) -> serde_json::Value {
    serde_json::json!({ "op": "get_node_css_properties", "selector": selector })
}

fn response_contains(fragment: &str) -> serde_json::Value {
    serde_json::json!({ "op": "assert_response", "contains": fragment })
}

fn response_lacks(fragment: &str) -> serde_json::Value {
    serde_json::json!({ "op": "assert_response", "not_contains": fragment })
}

// ---------------------------------------------------------------------------
// The selector names the node
// ---------------------------------------------------------------------------

#[test]
fn set_node_css_override_by_selector_changes_that_node_and_no_other() {
    let mut steps = mounted();
    steps.extend([
        serde_json::json!({
            "op": "set_node_css_override", "selector": "#a", "property": "width", "value": "100px"
        }),
        serde_json::json!({ "op": "wait_frame" }),
        props("#a"),
        response_contains("width: 100px"),
        props("#b"),
        response_lacks("width: 100px"),
    ]);
    assert_passes(&run("css_override_by_selector", false, steps));
}

#[test]
fn the_node_id_form_still_names_the_same_node() {
    // Node 5 is `#a` (see `DOM`). Both forms of every op reach it.
    let mut steps = mounted();
    steps.extend([
        serde_json::json!({
            "op": "set_node_css_override", "node_id": 5, "property": "width", "value": "100px"
        }),
        serde_json::json!({ "op": "wait_frame" }),
        props("#a"),
        response_contains("width: 100px"),
        serde_json::json!({ "op": "set_node_classes", "node_id": 5, "classes": ["hot"] }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "assert_exists", "selector": "#a.hot" }),
        serde_json::json!({ "op": "set_node_text", "node_id": 4, "text": "bye" }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "assert_text", "selector": "#label", "expected": "bye" }),
        serde_json::json!({ "op": "set_node_image", "node_id": 7, "width": 4, "height": 4,
                           "color": "#00ff00" }),
        serde_json::json!({ "op": "insert_node", "parent_id": 2, "node_type": "div",
                            "classes": ["new"] }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "assert_node_count", "selector": ".new", "expected": 1 }),
        serde_json::json!({ "op": "delete_node", "node_id": 6 }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "assert_not_exists", "selector": "#b" }),
    ]);
    assert_passes(&run("mutation_by_node_id", false, steps));
}

#[test]
fn set_node_classes_by_selector_keeps_the_id_and_replaces_the_classes() {
    let mut steps = mounted();
    steps.extend([
        serde_json::json!({ "op": "set_node_classes", "selector": "#a", "classes": ["hot"] }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "assert_exists", "selector": "#a.hot" }),
        serde_json::json!({ "op": "assert_not_exists", "selector": "#a.box" }),
        serde_json::json!({ "op": "assert_node_count", "selector": ".box", "expected": 1 }),
    ]);
    assert_passes(&run("classes_by_selector", false, steps));
}

#[test]
fn set_node_text_by_selector_retexts_the_text_node_of_the_element() {
    // `#label` is a `<p>`: the selector names the element, the op writes its text node.
    let mut steps = mounted();
    steps.extend([
        serde_json::json!({ "op": "set_node_text", "selector": "#label", "text": "bye" }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "assert_text", "selector": "#label", "expected": "bye" }),
    ]);
    assert_passes(&run("text_by_selector", false, steps));
}

#[test]
fn set_node_text_by_selector_refuses_an_element_that_has_no_single_text_node() {
    let errors = refusals(
        "text_by_selector_refused",
        vec![
            // A container of elements: nothing to retext.
            serde_json::json!({ "op": "set_node_text", "selector": "#a", "text": "x" }),
            // `one<b>x</b>two`: two text children, which one is meant?
            serde_json::json!({ "op": "set_node_text", "selector": "#mixed", "text": "x" }),
        ],
    );
    assert!(
        errors[0].contains("set_node_text") && errors[0].contains("no text node"),
        "{}",
        errors[0]
    );
    assert!(
        errors[1].contains("2 text nodes")
            && errors[1].contains("node 9")
            && errors[1].contains("node 12"),
        "{}",
        errors[1]
    );
}

#[test]
fn set_node_image_by_selector_swaps_the_image_and_refuses_a_div() {
    let mut steps = mounted();
    steps.extend([
        serde_json::json!({ "op": "set_node_image", "selector": "#pic", "width": 8, "height": 8,
                            "color": "#ff0000" }),
        serde_json::json!({ "op": "wait_frame" }),
    ]);
    assert_passes(&run("image_by_selector", false, steps));

    let errors = refusals(
        "image_by_selector_refused",
        vec![
            serde_json::json!({ "op": "set_node_image", "selector": "#a", "width": 8,
                                 "height": 8, "color": "#ff0000" }),
        ],
    );
    assert!(
        errors[0].contains("not a NodeType::Image node"),
        "{}",
        errors[0]
    );
}

#[test]
fn delete_node_by_selector_removes_that_node() {
    let mut steps = mounted();
    steps.extend([
        serde_json::json!({ "op": "delete_node", "selector": "#b" }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "assert_not_exists", "selector": "#b" }),
        serde_json::json!({ "op": "assert_exists", "selector": "#a" }),
        serde_json::json!({ "op": "assert_node_count", "selector": ".box", "expected": 1 }),
    ]);
    assert_passes(&run("delete_by_selector", false, steps));
}

#[test]
fn what_hung_below_a_deleted_node_is_no_longer_in_the_document() {
    // `delete_node` unlinks the node and leaves its children pointing at it: a
    // selector must not find them - they are not on screen and not in the tree.
    let steps = vec![
        serde_json::json!({ "op": "mount", "html": [
            "<body><div id=\"list\"><p class=\"row\">one</p><p class=\"row\">two</p></div>\
             <p class=\"row\" id=\"keep\">three</p></body>"
        ] }),
        serde_json::json!({ "op": "wait_frame" }),
        // three nodes carry the class `row`; two of them hang below `#list`
        serde_json::json!({ "op": "delete_node", "selector": "#list" }),
        serde_json::json!({ "op": "wait_frame" }),
        // only #keep is left in the document, so ".row" names exactly one node again
        // (counting the dead children it would be "several", and the op would refuse)
        serde_json::json!({ "op": "set_node_css_override", "selector": ".row",
                            "property": "width", "value": "77px" }),
        serde_json::json!({ "op": "wait_frame" }),
        props("#keep"),
        response_contains("width: 77px"),
    ];
    assert_passes(&run("selector_ignores_deleted_subtrees", false, steps));
}

#[test]
fn insert_node_by_parent_selector_appends_under_that_parent() {
    let mut steps = mounted();
    steps.extend([
        serde_json::json!({ "op": "insert_node", "parent_selector": "#root", "node_type": "div",
                            "classes": ["new"], "id": "n" }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "assert_exists", "selector": "#n" }),
        serde_json::json!({ "op": "assert_node_count", "selector": "#root > .new", "expected": 1 }),
    ]);
    assert_passes(&run("insert_by_parent_selector", false, steps));
}

#[test]
fn mouse_move_by_selector_moves_to_the_centre_of_the_box() {
    // `#a:hover` is red. The pointer is over `#a` only after it moved to the
    // middle of `#a`'s box, and off it after it moved to `#b`.
    let mut steps = mounted();
    steps.extend([
        props("#a"),
        response_contains("background-color: #0000ffff"),
        serde_json::json!({ "op": "mouse_move", "selector": "#a" }),
        serde_json::json!({ "op": "wait_frame" }),
        props("#a"),
        response_contains("background-color: #ff0000ff"),
        serde_json::json!({ "op": "mouse_move", "selector": "#b" }),
        serde_json::json!({ "op": "wait_frame" }),
        props("#a"),
        response_contains("background-color: #0000ffff"),
    ]);
    assert_passes(&run("mouse_move_by_selector", false, steps));
}

// ---------------------------------------------------------------------------
// Refusals, loud and by name
// ---------------------------------------------------------------------------

#[test]
fn a_selector_that_matches_nothing_is_refused_by_name() {
    let mut ops = mutation_ops(&serde_json::json!({ "selector": "#nope" }));
    ops.push(
        serde_json::json!({ "op": "insert_node", "parent_selector": "#nope",
                                 "node_type": "div" }),
    );
    ops.push(serde_json::json!({ "op": "mouse_move", "selector": "#nope" }));
    let labels: Vec<String> = ops
        .iter()
        .map(|op| op["op"].as_str().unwrap_or_default().to_string())
        .collect();
    for (label, error) in labels.iter().zip(refusals("selector_matches_nothing", ops)) {
        assert!(
            error.contains(label.as_str()) && error.contains("no node matches '#nope'"),
            "{label}: {error}"
        );
    }
}

#[test]
fn a_selector_that_matches_several_nodes_is_refused_and_says_which() {
    let mut ops = mutation_ops(&serde_json::json!({ "selector": ".box" }));
    ops.push(
        serde_json::json!({ "op": "insert_node", "parent_selector": ".box",
                                 "node_type": "div" }),
    );
    ops.push(serde_json::json!({ "op": "mouse_move", "selector": ".box" }));
    let labels: Vec<String> = ops
        .iter()
        .map(|op| op["op"].as_str().unwrap_or_default().to_string())
        .collect();
    for (label, error) in labels.iter().zip(refusals("selector_matches_several", ops)) {
        assert!(
            error.contains(label.as_str())
                && error.contains("'.box' matches 2 nodes")
                && error.contains("node 5")
                && error.contains("node 6"),
            "{label}: {error}"
        );
    }
}

#[test]
fn a_target_is_a_selector_or_a_node_id_never_both_never_neither() {
    let both = serde_json::json!({ "selector": "#a", "node_id": 5 });
    let mut ops = mutation_ops(&both);
    ops.push(
        serde_json::json!({ "op": "insert_node", "parent_selector": "#root",
                                 "parent_id": 2, "node_type": "div" }),
    );
    ops.push(serde_json::json!({ "op": "mouse_move", "selector": "#a", "x": 1.0, "y": 2.0 }));
    let n = ops.len();
    let both_errors = refusals("target_both", ops);
    assert_eq!(both_errors.len(), n);
    for error in &both_errors[..5] {
        assert!(
            error.contains("exactly one of `selector` / `node_id`"),
            "{error}"
        );
    }
    assert!(
        both_errors[5].contains("exactly one of `parent_selector` / `parent_id`"),
        "{}",
        both_errors[5]
    );
    assert!(
        both_errors[6].contains("`selector` or `x` + `y`"),
        "{}",
        both_errors[6]
    );

    let neither = serde_json::json!({});
    let mut ops = mutation_ops(&neither);
    ops.push(serde_json::json!({ "op": "insert_node", "node_type": "div" }));
    ops.push(serde_json::json!({ "op": "mouse_move" }));
    ops.push(serde_json::json!({ "op": "mouse_move", "x": 1.0 }));
    let neither_errors = refusals("target_neither", ops);
    for error in &neither_errors[..5] {
        assert!(error.contains("needs a `selector`"), "{error}");
    }
    assert!(
        neither_errors[5].contains("needs a `parent_selector`"),
        "{}",
        neither_errors[5]
    );
    assert!(
        neither_errors[6].contains("`x` + `y`"),
        "{}",
        neither_errors[6]
    );
    assert!(
        neither_errors[7].contains("`x` + `y`"),
        "{}",
        neither_errors[7]
    );
}

#[test]
fn a_selector_the_engine_cannot_parse_is_not_reported_as_a_miss() {
    let errors = refusals(
        "selector_unparsable",
        vec![
            serde_json::json!({ "op": "set_node_css_override", "selector": "div:nosuchpseudo",
                                "property": "width", "value": "1px" }),
            serde_json::json!({ "op": "mouse_move", "selector": "" }),
        ],
    );
    assert!(
        errors[0].contains("not a valid CSS selector"),
        "{}",
        errors[0]
    );
    assert!(!errors[0].contains("no node matches"), "{}", errors[0]);
    assert!(
        errors[1].contains("not a valid CSS selector"),
        "{}",
        errors[1]
    );
}

#[test]
fn mouse_move_by_selector_refuses_a_node_without_a_box() {
    let errors = refusals(
        "mouse_move_boxless",
        vec![serde_json::json!({ "op": "mouse_move", "selector": "#hidden" })],
    );
    assert!(
        errors[0].contains("mouse_move") && errors[0].contains("no box"),
        "{}",
        errors[0]
    );
}

#[test]
fn a_selector_before_the_document_exists_says_to_wait_for_the_mount() {
    // `mount` is applied at the next frame: the step right after it still sees no document.
    let steps = vec![
        serde_json::json!({ "op": "mount", "html": [DOM], "css": css() }),
        serde_json::json!({ "op": "set_node_css_override", "selector": "#a", "property": "width",
                            "value": "1px" }),
    ];
    let result = run("selector_before_mount_lands", true, steps);
    let step = result
        .steps
        .iter()
        .find(|s| s.step_index == 1)
        .expect("step 1 ran");
    assert_eq!(step.status, "fail", "{}", failures(&result));
    let error = step.error.clone().unwrap_or_default();
    assert!(error.contains("wait_frame"), "{error}");
}
