//! The AzBuilder server messages, driven end to end through the REAL op
//! dispatch (`process_debug_event` + the scenario runner) on a headless
//! window: every message the builder UI sends for drag and drop (insert at a
//! position, move, delete, undo / redo), for "convert subtree to component"
//! and for the palette previews.
//!
//! Each scenario checks two things: the op's own response (the document the
//! UI renders its tree from) and the NATIVE DOM the window shows afterwards
//! (`assert_exists` / `assert_dom` read the live `StyledDom`), because the bug
//! these pin was exactly a UI whose edits never reached the window.
//!
//! uid allocation, which the scenarios rely on: the document root (`<body>`)
//! is uid 0, and every `builder_insert` allocates exactly ONE new uid, counting
//! up from 1 (an element's text lives in its `text` attribute, not in a child
//! node, so it takes no uid of its own). A converted subtree's instance keeps
//! the uid of the node it replaced.

use super::{run_e2e_test, E2eTest, E2eTestResult};

/// Run `steps` as one headless scenario on a 400x300 window.
fn run(name: &str, continue_on_failure: bool, steps: serde_json::Value) -> E2eTestResult {
    let test: E2eTest = serde_json::from_value(serde_json::json!({
        "name": name,
        "config": { "continue_on_failure": continue_on_failure },
        "setup": { "window_width": 400, "window_height": 300, "dpi": 96 },
        "steps": steps,
    }))
    .expect("the scenario literal is a valid E2eTest");
    run_e2e_test(&test)
}

/// Every failing step, one line each — the assertion message a red run prints.
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

/// The frame barrier every scenario in `e2e/` puts between a DOM change and
/// the assertions about it.
fn settle() -> [serde_json::Value; 2] {
    [
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "wait", "ms": 100 }),
    ]
}

fn steps(parts: Vec<serde_json::Value>) -> serde_json::Value {
    serde_json::Value::Array(parts)
}

#[test]
fn a_palette_drop_inserts_the_component_at_the_drop_position_and_the_native_window_shows_it() {
    let [wf, w] = settle();
    let result = run(
        "builder_insert_at_position",
        false,
        steps(vec![
            // Append to <body> (uid 0): uid 1.
            serde_json::json!({ "op": "builder_insert", "parent": 0, "component": "p",
                                "attrs": { "id": "a", "text": "first" } }),
            // The response IS the refreshed tree the UI renders.
            serde_json::json!({ "op": "assert_response", "contains": "\"inserted\":1" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"tag\":\"p\"" }),
            // Drop BEFORE the first row: index 0 of <body>. uid 2.
            serde_json::json!({ "op": "builder_insert", "parent": 0, "index": 0, "component": "p",
                                "attrs": { "id": "z", "text": "zeroth" } }),
            // Drop AFTER the last row: append. uid 3.
            serde_json::json!({ "op": "builder_insert", "parent": 0, "library": "builtin",
                                "component": "div", "attrs": { "id": "box" } }),
            // Drop INTO a row: parent = that row's uid. uid 4.
            serde_json::json!({ "op": "builder_insert", "parent": 3, "component": "span",
                                "attrs": { "id": "s", "text": "inner" } }),
            wf.clone(),
            w.clone(),
            serde_json::json!({ "op": "assert_exists", "selector": "#z:nth-child(1)" }),
            serde_json::json!({ "op": "assert_exists", "selector": "#a:nth-child(2)" }),
            serde_json::json!({ "op": "assert_exists", "selector": "#box:nth-child(3)" }),
            serde_json::json!({ "op": "assert_exists", "selector": "#box > #s" }),
            serde_json::json!({ "op": "assert_dom", "contains": "zeroth" }),
            serde_json::json!({ "op": "assert_dom", "contains": "inner" }),
            serde_json::json!({ "op": "builder_get_document" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"uid\":4" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"active\":true" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"can_undo\":true" }),
        ]),
    );
    assert_passes(&result);
}

#[test]
fn moving_a_tree_node_reorders_the_document_and_the_native_window() {
    let [wf, w] = settle();
    let result = run(
        "builder_move",
        false,
        steps(vec![
            serde_json::json!({ "op": "builder_insert", "parent": 0, "component": "p",
                                "attrs": { "id": "a", "text": "A" } }),
            serde_json::json!({ "op": "builder_insert", "parent": 0, "component": "p",
                                "attrs": { "id": "b", "text": "B" } }),
            serde_json::json!({ "op": "builder_insert", "parent": 0, "component": "div",
                                "attrs": { "id": "box" } }),
            // b BEFORE a: the index is the drop indicator's slot among the
            // parent's children as the user SEES them, i.e. before the move.
            serde_json::json!({ "op": "builder_move", "node": 2, "parent": 0, "index": 0 }),
            wf.clone(),
            w.clone(),
            serde_json::json!({ "op": "assert_exists", "selector": "#b:nth-child(1)" }),
            serde_json::json!({ "op": "assert_exists", "selector": "#a:nth-child(2)" }),
            // a INTO box (no index: append).
            serde_json::json!({ "op": "builder_move", "node": 1, "parent": 3 }),
            wf.clone(),
            w.clone(),
            serde_json::json!({ "op": "assert_exists", "selector": "#box > #a" }),
            serde_json::json!({ "op": "assert_node_count", "selector": "body > p", "expected": 1 }),
            // b AFTER box, within the same parent: [b, box] -> slot 2 -> [box, b].
            serde_json::json!({ "op": "builder_move", "node": 2, "parent": 0, "index": 2 }),
            wf.clone(),
            w.clone(),
            serde_json::json!({ "op": "assert_exists", "selector": "#box:nth-child(1)" }),
            serde_json::json!({ "op": "assert_exists", "selector": "#b:nth-child(2)" }),
            // And a move is one undo step.
            serde_json::json!({ "op": "builder_undo" }),
            wf,
            w,
            serde_json::json!({ "op": "assert_exists", "selector": "#b:nth-child(1)" }),
            serde_json::json!({ "op": "assert_exists", "selector": "#box:nth-child(2)" }),
        ]),
    );
    assert_passes(&result);
}

#[test]
fn deleting_a_node_removes_it_from_the_window_and_undo_redo_replay_it() {
    let [wf, w] = settle();
    let result = run(
        "builder_delete_undo_redo",
        false,
        steps(vec![
            serde_json::json!({ "op": "builder_insert", "parent": 0, "component": "p",
                                "attrs": { "id": "a", "text": "A" } }),
            serde_json::json!({ "op": "builder_insert", "parent": 0, "component": "p",
                                "attrs": { "id": "b", "text": "B" } }),
            wf.clone(),
            w.clone(),
            serde_json::json!({ "op": "assert_exists", "selector": "#a:nth-child(1)" }),
            // The FIRST child: the case the old `delete_node` left as an empty
            // div, because the flat hierarchy derives first child = id + 1.
            serde_json::json!({ "op": "builder_delete", "node": 1 }),
            serde_json::json!({ "op": "assert_response", "contains": "\"can_undo\":true" }),
            wf.clone(),
            w.clone(),
            serde_json::json!({ "op": "assert_not_exists", "selector": "#a" }),
            serde_json::json!({ "op": "assert_exists", "selector": "#b:nth-child(1)" }),
            serde_json::json!({ "op": "builder_undo" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"can_redo\":true" }),
            wf.clone(),
            w.clone(),
            serde_json::json!({ "op": "assert_exists", "selector": "#a:nth-child(1)" }),
            serde_json::json!({ "op": "assert_exists", "selector": "#b:nth-child(2)" }),
            serde_json::json!({ "op": "builder_redo" }),
            wf,
            w,
            serde_json::json!({ "op": "assert_not_exists", "selector": "#a" }),
        ]),
    );
    assert_passes(&result);
}

#[test]
fn the_builder_refuses_edits_that_would_corrupt_the_tree_and_says_why() {
    let result = run(
        "builder_refusals",
        true,
        steps(vec![
            /* 0 */
            serde_json::json!({ "op": "builder_insert", "parent": 0, "component": "div",
                                "attrs": { "id": "outer" } }),
            /* 1 */
            serde_json::json!({ "op": "builder_insert", "parent": 1, "component": "div",
                                "attrs": { "id": "inner" } }),
            /* 2: a node cannot move into its own subtree */
            serde_json::json!({ "op": "builder_move", "node": 1, "parent": 2 }),
            /* 3: the document root is not deletable */
            serde_json::json!({ "op": "builder_delete", "node": 0 }),
            /* 4: an unknown parent */
            serde_json::json!({ "op": "builder_insert", "parent": 99, "component": "p" }),
            /* 5: an unknown component */
            serde_json::json!({ "op": "builder_insert", "parent": 0, "library": "nope",
                                "component": "x" }),
            /* 6 */
            serde_json::json!({ "op": "builder_get_document" }),
            /* 7: none of the refused edits changed the document */
            serde_json::json!({ "op": "assert_response", "contains": "\"uid\":2" }),
            /* 8 */
            serde_json::json!({ "op": "assert_response", "not_contains": "\"uid\":3" }),
        ]),
    );
    let status = |i: usize| {
        result
            .steps
            .iter()
            .find(|s| s.step_index == i)
            .map(|s| (s.status.clone(), s.error.clone().unwrap_or_default()))
            .unwrap_or_else(|| ("missing".to_string(), String::new()))
    };
    for i in [0, 1, 6, 7, 8] {
        assert_eq!(
            status(i).0,
            "pass",
            "step {i} must pass:\n{}",
            failures(&result)
        );
    }
    for (i, needle) in [(2, "descendant"), (3, "root"), (4, "99"), (5, "nope")] {
        let (st, err) = status(i);
        assert_eq!(
            st,
            "fail",
            "step {i} must be refused:\n{}",
            failures(&result)
        );
        assert!(
            err.contains(needle),
            "step {i}'s refusal must say why (expected '{needle}' in: {err})"
        );
    }
}

#[test]
fn a_selected_subtree_converts_into_a_component_that_the_palette_lists_and_that_drops_again() {
    let [wf, w] = settle();
    let result = run(
        "builder_convert_to_component",
        false,
        steps(vec![
            serde_json::json!({ "op": "builder_insert", "parent": 0, "component": "div",
                                "attrs": { "class": "card" } }),
            serde_json::json!({ "op": "builder_insert", "parent": 1, "component": "h1",
                                "attrs": { "text": "Title" } }),
            serde_json::json!({ "op": "builder_insert", "parent": 1, "component": "a",
                                "attrs": { "text": "More", "href": "https://example.com" } }),
            // The library does not exist yet: convert creates it.
            serde_json::json!({ "op": "builder_convert_to_component", "node": 1,
                                "library": "user", "name": "card" }),
            // The subtree was replaced by an instance of the new component,
            // which keeps the uid of the node it replaced.
            serde_json::json!({ "op": "assert_response", "contains": "\"kind\":\"component\"" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"library\":\"user\"" }),
            wf.clone(),
            w.clone(),
            // The instance expands to the same DOM the subtree was.
            serde_json::json!({ "op": "assert_node_count", "selector": ".card", "expected": 1 }),
            serde_json::json!({ "op": "assert_dom", "contains": "Title" }),
            // The palette lists it, with parameters inferred from the subtree.
            serde_json::json!({ "op": "get_library_components", "library": "user" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"tag\":\"card\"" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"name\":\"text\"" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"name\":\"text_2\"" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"name\":\"href\"" }),
            // ...and it drops again, with an argument for the inferred parameter.
            serde_json::json!({ "op": "builder_insert", "parent": 0, "library": "user",
                                "component": "card", "attrs": { "text": "Second" } }),
            wf,
            w,
            serde_json::json!({ "op": "assert_node_count", "selector": ".card", "expected": 2 }),
            serde_json::json!({ "op": "assert_node_count", "selector": "h1", "expected": 2 }),
            serde_json::json!({ "op": "assert_dom", "contains": "Second" }),
            serde_json::json!({ "op": "assert_dom", "contains": "Title" }),
            // The component's tree is the TEMPLATE, placeholders and all.
            serde_json::json!({ "op": "get_component_render_tree", "library": "user",
                                "name": "card" }),
            serde_json::json!({ "op": "assert_response", "contains": "{text}" }),
        ]),
    );
    assert_passes(&result);
}

/// The UI's older convert path (the live-DOM context menu) and the component
/// detail's mini-tree editor both send `render_tree`, which `create_component`
/// / `update_component` used to drop without a word.
#[test]
fn create_component_with_a_render_tree_stores_it_as_the_component_template() {
    let [wf, w] = settle();
    let result = run(
        "create_component_render_tree",
        false,
        steps(vec![
            serde_json::json!({ "op": "create_library", "name": "lib2" }),
            serde_json::json!({ "op": "create_component", "library": "lib2", "name": "badge",
                                "description": "Created from DOM subtree",
                                "render_tree": { "tag": "span", "classes": ["badge"],
                                    "children": [ { "tag": "__text__", "text": "New" } ] } }),
            serde_json::json!({ "op": "get_component_render_tree", "library": "lib2",
                                "name": "badge" }),
            serde_json::json!({ "op": "assert_response", "contains": "badge" }),
            serde_json::json!({ "op": "assert_response", "contains": "{text}" }),
            serde_json::json!({ "op": "get_library_components", "library": "lib2" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"name\":\"text\"" }),
            serde_json::json!({ "op": "assert_response",
                                "contains": "Created from DOM subtree" }),
            serde_json::json!({ "op": "builder_insert", "parent": 0, "library": "lib2",
                                "component": "badge" }),
            wf,
            w,
            serde_json::json!({ "op": "assert_node_count", "selector": ".badge", "expected": 1 }),
            serde_json::json!({ "op": "assert_dom", "contains": "New" }),
        ]),
    );
    assert_passes(&result);
}

#[test]
fn update_component_with_a_render_tree_replaces_the_component_template() {
    let [wf, w] = settle();
    let result = run(
        "update_component_render_tree",
        false,
        steps(vec![
            serde_json::json!({ "op": "create_library", "name": "lib3" }),
            serde_json::json!({ "op": "create_component", "library": "lib3", "name": "panel" }),
            // The mini tree sends its ROOT LIST (an array), with element text
            // on the element itself.
            serde_json::json!({ "op": "update_component", "library": "lib3", "name": "panel",
                                "render_tree": [ { "tag": "div", "classes": ["panel-box"],
                                    "children": [ { "tag": "p", "text": "hello",
                                                    "children": [] } ] } ] }),
            serde_json::json!({ "op": "get_component_render_tree", "library": "lib3",
                                "name": "panel" }),
            serde_json::json!({ "op": "assert_response", "contains": "panel-box" }),
            serde_json::json!({ "op": "builder_insert", "parent": 0, "library": "lib3",
                                "component": "panel" }),
            wf,
            w,
            serde_json::json!({ "op": "assert_node_count", "selector": ".panel-box",
                                "expected": 1 }),
            serde_json::json!({ "op": "assert_dom", "contains": "hello" }),
        ]),
    );
    assert_passes(&result);
}

#[test]
fn a_component_thumbnail_is_rendered_natively_and_cached_until_the_component_changes() {
    let result = run(
        "component_thumbnail_cache",
        false,
        steps(vec![
            serde_json::json!({ "op": "create_library", "name": "thumbs" }),
            serde_json::json!({ "op": "create_component", "library": "thumbs", "name": "sq",
                                "render_tree": { "tag": "div", "classes": ["sq"] } }),
            serde_json::json!({ "op": "update_component", "library": "thumbs", "name": "sq",
                                "css": ".sq { width: 24px; height: 24px; background: #ff0000; }" }),
            serde_json::json!({ "op": "get_component_thumbnail", "library": "thumbs",
                                "name": "sq" }),
            // A PNG (base64 of the 8-byte signature), rendered by the CPU renderer.
            serde_json::json!({ "op": "assert_response", "contains": "data:image/png;base64,iVBORw0KGgo" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"cached\":false" }),
            serde_json::json!({ "op": "get_component_thumbnail", "library": "thumbs",
                                "name": "sq" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"cached\":true" }),
            // A changed component is a different picture: the cache must miss.
            serde_json::json!({ "op": "update_component", "library": "thumbs", "name": "sq",
                                "css": ".sq { width: 30px; height: 30px; background: #00ff00; }" }),
            serde_json::json!({ "op": "get_component_thumbnail", "library": "thumbs",
                                "name": "sq" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"cached\":false" }),
        ]),
    );
    assert_passes(&result);
}

#[test]
fn builder_reset_gives_the_window_back_to_the_app() {
    let [wf, w] = settle();
    let result = run(
        "builder_reset",
        false,
        steps(vec![
            serde_json::json!({ "op": "builder_insert", "parent": 0, "component": "p",
                                "attrs": { "id": "a", "text": "A" } }),
            wf.clone(),
            w.clone(),
            serde_json::json!({ "op": "assert_exists", "selector": "#a" }),
            serde_json::json!({ "op": "builder_reset" }),
            wf,
            w,
            serde_json::json!({ "op": "assert_not_exists", "selector": "#a" }),
            serde_json::json!({ "op": "builder_get_document" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"active\":false" }),
        ]),
    );
    assert_passes(&result);
}

// ── B5: the document's own stylesheet ──

#[test]
fn the_documents_own_stylesheet_styles_the_window_beats_component_css_survives_a_remount_and_undoes()
{
    let [wf, w] = settle();
    let result = run(
        "builder_document_stylesheet",
        false,
        steps(vec![
            serde_json::json!({ "op": "create_library", "name": "lib8" }),
            serde_json::json!({ "op": "create_component", "library": "lib8", "name": "box",
                                "render_tree": { "tag": "div", "classes": ["box"] } }),
            serde_json::json!({ "op": "update_component", "library": "lib8", "name": "box",
                                "css": ".box { width: 50px; height: 10px; }" }),
            // uid 1: an instance; its `id` goes on the component's root.
            serde_json::json!({ "op": "builder_insert", "parent": 0, "library": "lib8",
                                "component": "box", "attrs": { "id": "b" } }),
            wf.clone(),
            w.clone(),
            serde_json::json!({ "op": "assert_layout", "selector": "#b", "property": "width",
                                "expected": 50, "tolerance": 1 }),
            // Nothing set yet: the document's stylesheet is empty.
            serde_json::json!({ "op": "builder_get_stylesheet" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"stylesheet\":\"\"" }),
            // The same selector as the component's: the document's sheet comes
            // later in the cascade and wins, as an app stylesheet does.
            serde_json::json!({ "op": "builder_set_stylesheet", "css": ".box { width: 123px; }" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"can_undo\":true" }),
            serde_json::json!({ "op": "assert_response", "contains": "width: 123px" }),
            wf.clone(),
            w.clone(),
            serde_json::json!({ "op": "assert_layout", "selector": "#b", "property": "width",
                                "expected": 123, "tolerance": 1 }),
            // Another edit re-mounts the document: the sheet is part of it.
            serde_json::json!({ "op": "builder_insert", "parent": 0, "component": "p",
                                "attrs": { "text": "after" } }),
            wf.clone(),
            w.clone(),
            serde_json::json!({ "op": "assert_dom", "contains": "after" }),
            serde_json::json!({ "op": "assert_layout", "selector": "#b", "property": "width",
                                "expected": 123, "tolerance": 1 }),
            serde_json::json!({ "op": "builder_get_stylesheet" }),
            serde_json::json!({ "op": "assert_response", "contains": ".box { width: 123px; }" }),
            // Undo the paragraph, then the stylesheet: one step each.
            serde_json::json!({ "op": "builder_undo" }),
            serde_json::json!({ "op": "builder_undo" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"stylesheet\":\"\"" }),
            wf.clone(),
            w.clone(),
            serde_json::json!({ "op": "assert_layout", "selector": "#b", "property": "width",
                                "expected": 50, "tolerance": 1 }),
            serde_json::json!({ "op": "builder_redo" }),
            wf,
            w,
            serde_json::json!({ "op": "assert_layout", "selector": "#b", "property": "width",
                                "expected": 123, "tolerance": 1 }),
        ]),
    );
    assert_passes(&result);
}

// ── B5: drops onto the window canvas ──

/// The `value` of step `i`'s JSON answer (the step must have passed).
fn answer(result: &E2eTestResult, i: usize) -> serde_json::Value {
    let step = result
        .steps
        .iter()
        .find(|s| s.step_index == i)
        .unwrap_or_else(|| panic!("no step {i}:\n{}", failures(result)));
    assert_eq!(
        step.status,
        "pass",
        "step {i} `{}` must pass:\n{}",
        step.op,
        failures(result)
    );
    step.response
        .as_ref()
        .and_then(|r| r.get("value"))
        .cloned()
        .unwrap_or_else(|| panic!("step {i} `{}` answered no data", step.op))
}

#[test]
fn a_point_in_the_window_hit_tests_to_the_document_node_under_it_through_its_marker() {
    let [wf, w] = settle();
    let result = run(
        "builder_hit_test",
        true,
        steps(vec![
            /* 0 */
            serde_json::json!({ "op": "builder_set_stylesheet",
                "css": "body { margin: 0px; padding: 0px; } #a { height: 40px; } #b { height: 40px; }" }),
            /* 1: uid 1 */
            serde_json::json!({ "op": "builder_insert", "parent": 0, "component": "div",
                                "attrs": { "id": "a" } }),
            /* 2: uid 2 */
            serde_json::json!({ "op": "builder_insert", "parent": 0, "component": "div",
                                "attrs": { "id": "b" } }),
            /* 3 */ serde_json::json!({ "op": "create_library", "name": "lib9" }),
            /* 4 */
            serde_json::json!({ "op": "create_component", "library": "lib9", "name": "card",
                                "render_tree": { "tag": "div", "classes": ["card"],
                                    "children": [ { "tag": "p", "text": "inner" } ] } }),
            /* 5 */
            serde_json::json!({ "op": "update_component", "library": "lib9", "name": "card",
                                "css": ".card { height: 60px; }" }),
            /* 6: uid 3, an instance: only its root carries the marker */
            serde_json::json!({ "op": "builder_insert", "parent": 0, "library": "lib9",
                                "component": "card" }),
            /* 7 */ wf,
            /* 8 */ w,
            /* 9: the middle of #a */
            serde_json::json!({ "op": "builder_hit_test", "x": 10, "y": 20 }),
            /* 10: the top quarter of #b */
            serde_json::json!({ "op": "builder_hit_test", "x": 10, "y": 50 }),
            /* 11: inside the card, over its <p> or its own box */
            serde_json::json!({ "op": "builder_hit_test", "x": 10, "y": 110 }),
        ]),
    );
    let a = answer(&result, 9);
    assert_eq!(a["hit"], true, "{a}");
    assert_eq!(a["uid"], 1, "{a}");
    let rel = |v: &serde_json::Value, k: &str| v[k].as_f64().unwrap_or(-1.0);
    assert!((rel(&a, "rel_y") - 0.5).abs() < 0.05, "the middle of #a: {a}");
    assert!(
        a["rect"]["height"].as_f64().is_some_and(|h| (h - 40.0).abs() < 1.0),
        "the rect of the marked node, in window coordinates: {a}"
    );
    let b = answer(&result, 10);
    assert_eq!(b["uid"], 2, "{b}");
    assert!((rel(&b, "rel_y") - 0.25).abs() < 0.05, "the top quarter of #b: {b}");
    let card = answer(&result, 11);
    assert_eq!(
        card["uid"], 3,
        "a node inside an instance belongs to the instance: {card}"
    );
}

// ── B5: the builder's markers stay out of the inspector ──

#[test]
fn the_live_dom_answers_a_mounted_nodes_marker_as_builder_uid_not_as_a_class() {
    let [wf, w] = settle();
    let result = run(
        "builder_markers_hidden",
        false,
        steps(vec![
            serde_json::json!({ "op": "builder_insert", "parent": 0, "component": "p",
                                "attrs": { "id": "a", "class": "note azb-card", "text": "A" } }),
            wf,
            w,
            // The engine still has the marker (the builder's hit test and the
            // Inspector's lookup need it)...
            serde_json::json!({ "op": "assert_exists", "selector": ".azb-1" }),
            // ...but the Live DOM tree and the Inspector's class list do not
            // show it: it is answered as `builder_uid`.
            serde_json::json!({ "op": "get_node_hierarchy" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"builder_uid\":1" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"builder_uid\":0" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"note\"" }),
            // An ordinary class that happens to start with `azb-` stays.
            serde_json::json!({ "op": "assert_response", "contains": "\"azb-card\"" }),
            serde_json::json!({ "op": "assert_response", "not_contains": "\"azb-1\"" }),
            serde_json::json!({ "op": "assert_response", "not_contains": "\"azb-0\"" }),
        ]),
    );
    assert_passes(&result);
}

// ── B5: duplicate, and the document as a file ──

#[test]
fn duplicating_a_node_copies_its_subtree_with_fresh_uids_right_after_it_as_one_undo_step() {
    let [wf, w] = settle();
    let result = run(
        "builder_duplicate",
        false,
        steps(vec![
            // uid 1: div.box > [p "A" (2), span "B" (3)]; uid 4: p#tail.
            serde_json::json!({ "op": "builder_insert", "parent": 0, "component": "div",
                                "attrs": { "class": "box" } }),
            serde_json::json!({ "op": "builder_insert", "parent": 1, "component": "p",
                                "attrs": { "text": "A" } }),
            serde_json::json!({ "op": "builder_insert", "parent": 1, "component": "span",
                                "attrs": { "text": "B" } }),
            serde_json::json!({ "op": "builder_insert", "parent": 0, "component": "p",
                                "attrs": { "id": "tail", "text": "tail" } }),
            // The copy's root is uid 5, its children 6 and 7 (DFS).
            serde_json::json!({ "op": "builder_duplicate", "node": 1 }),
            serde_json::json!({ "op": "assert_response", "contains": "\"inserted\":5" }),
            serde_json::json!({ "op": "assert_response", "contains": "\"uid\":7" }),
            serde_json::json!({ "op": "assert_response", "not_contains": "\"uid\":8" }),
            wf.clone(),
            w.clone(),
            serde_json::json!({ "op": "assert_node_count", "selector": ".box", "expected": 2 }),
            serde_json::json!({ "op": "assert_node_count", "selector": ".box > span", "expected": 2 }),
            // Right after the original: div, its copy, then p#tail.
            serde_json::json!({ "op": "assert_exists", "selector": "#tail:nth-child(3)" }),
            serde_json::json!({ "op": "assert_exists", "selector": ".azb-5" }),
            // The copy is a node of its own: editing it leaves the original.
            serde_json::json!({ "op": "builder_set_attribute", "node": 7, "name": "text",
                                "value": "copy" }),
            wf.clone(),
            w.clone(),
            serde_json::json!({ "op": "assert_dom", "contains": "copy" }),
            serde_json::json!({ "op": "assert_dom", "contains": "B" }),
            // Undo the edit, then the duplicate.
            serde_json::json!({ "op": "builder_undo" }),
            serde_json::json!({ "op": "builder_undo" }),
            wf,
            w,
            serde_json::json!({ "op": "assert_node_count", "selector": ".box", "expected": 1 }),
        ]),
    );
    assert_passes(&result);
}

#[test]
fn the_document_saves_as_json_and_loads_back_as_one_undoable_edit() {
    let [wf, w] = settle();
    let file = serde_json::json!({
        "format": "azul-builder-document",
        "version": 1,
        "root": { "kind": "element", "tag": "body", "attrs": {}, "children": [
            { "kind": "element", "tag": "p", "attrs": { "id": "b", "text": "B" }, "children": [] }
        ] },
        "stylesheet": "#b { width: 77px; }",
    });
    let result = run(
        "builder_save_load_document",
        true,
        steps(vec![
            /* 0 */
            serde_json::json!({ "op": "builder_insert", "parent": 0, "component": "p",
                                "attrs": { "id": "a", "text": "A" } }),
            /* 1 */
            serde_json::json!({ "op": "builder_set_stylesheet", "css": "#a { width: 123px; }" }),
            /* 2: the file project_save writes as document.json */
            serde_json::json!({ "op": "builder_save_document" }),
            /* 3 */
            serde_json::json!({ "op": "assert_response", "contains": "\"format\":\"azul-builder-document\"" }),
            /* 4 */
            serde_json::json!({ "op": "assert_response", "contains": "#a { width: 123px; }" }),
            /* 5: uids are the session's, not the file's */
            serde_json::json!({ "op": "assert_response", "not_contains": "\"uid\"" }),
            /* 6: another document replaces this one... */
            serde_json::json!({ "op": "builder_load_document", "document": file }),
            /* 7: ...as an edit: it can be undone */
            serde_json::json!({ "op": "assert_response", "contains": "\"can_undo\":true" }),
            /* 8 */ wf.clone(),
            /* 9 */ w.clone(),
            /* 10 */ serde_json::json!({ "op": "assert_not_exists", "selector": "#a" }),
            /* 11 */
            serde_json::json!({ "op": "assert_layout", "selector": "#b", "property": "width",
                                "expected": 77, "tolerance": 1 }),
            /* 12 */ serde_json::json!({ "op": "builder_undo" }),
            /* 13 */ wf,
            /* 14 */ w,
            /* 15 */
            serde_json::json!({ "op": "assert_layout", "selector": "#a", "property": "width",
                                "expected": 123, "tolerance": 1 }),
            /* 16 */ serde_json::json!({ "op": "assert_not_exists", "selector": "#b" }),
            /* 17: refusals, with reasons */
            serde_json::json!({ "op": "builder_load_document",
                                "document": { "format": "azul-project", "root": {} } }),
            /* 18 */ serde_json::json!({ "op": "builder_duplicate", "node": 0 }),
            /* 19 */ serde_json::json!({ "op": "builder_duplicate", "node": 99 }),
        ]),
    );
    let status = |i: usize| {
        result
            .steps
            .iter()
            .find(|s| s.step_index == i)
            .map(|s| (s.status.clone(), s.error.clone().unwrap_or_default()))
            .unwrap_or_else(|| ("missing".to_string(), String::new()))
    };
    for i in 0..=16 {
        assert_eq!(status(i).0, "pass", "step {i} must pass:\n{}", failures(&result));
    }
    for (i, needle) in [(17, "format"), (18, "root"), (19, "99")] {
        let (st, err) = status(i);
        assert_eq!(st, "fail", "step {i} must be refused:\n{}", failures(&result));
        assert!(err.contains(needle), "step {i}: expected '{needle}' in: {err}");
    }
}

// ── B7: every builtin's palette preview ──

/// The builtins that have nothing to show on their own: their palette card
/// says "no visual" (with the reason the server gives) instead of an empty
/// box. Everything else in `register_builtin_components` must PREVIEW.
const NO_VISUAL_BUILTINS: &[&str] = &[
    // The document's structure and <head> content (not in the palette).
    "html", "head", "title", "body", "meta", "link", "script", "style", "base",
    // A break and the table's columns: no box of their own.
    "br", "wbr", "pagebreak", "col", "colgroup",
    // Shown only inside a <select> / as an <input list>'s suggestions.
    "option", "optgroup", "datalist",
    // They show what their source names: nothing without one.
    "canvas", "object", "embed", "audio", "video",
    // Parts of another element.
    "param", "source", "track", "map", "area",
    // Not drawn by azul (yet).
    "progress", "meter",
];

#[test]
fn every_visual_builtin_has_a_palette_preview_and_every_other_says_why_it_has_none() {
    const STEPS_PER_NAME: usize = 3;

    let library = azul_core::xml::register_builtin_components();
    // `builtin:map` names two builtins (the image map and the structural
    // map); the op resolves the name to the first, the image map.
    let mut seen = std::collections::BTreeSet::new();
    let names: Vec<String> = library
        .components
        .as_ref()
        .iter()
        .map(|c| c.id.name.as_str().to_string())
        .filter(|n| seen.insert(n.clone()))
        .collect();
    assert!(names.len() > 100, "the builtin library lists its elements");

    let mut parts = Vec::new();
    for name in &names {
        parts.push(serde_json::json!({ "op": "get_component_thumbnail", "library": "builtin",
                                       "name": name, "width": 140, "dpi": 1 }));
        if NO_VISUAL_BUILTINS.contains(&name.as_str()) {
            parts.push(serde_json::json!({ "op": "assert_response", "contains": "\"empty\":true" }));
            parts.push(serde_json::json!({ "op": "assert_response", "contains": "\"no_visual\":\"" }));
        } else {
            // A PNG (base64 of its 8-byte signature): the render had pixels.
            parts.push(serde_json::json!({ "op": "assert_response",
                                           "contains": "data:image/png;base64,iVBORw0KGgo" }));
            parts.push(serde_json::json!({ "op": "assert_response", "contains": "\"no_visual\":null" }));
        }
    }
    let result = run("builtin_palette_previews", true, steps(parts));

    let mut wrong: Vec<String> = result
        .steps
        .iter()
        .filter(|s| s.status != "pass")
        .filter_map(|s| names.get(s.step_index / STEPS_PER_NAME))
        .map(|n| {
            let want = if NO_VISUAL_BUILTINS.contains(&n.as_str()) {
                "\"no visual\" with a reason"
            } else {
                "a preview"
            };
            format!("<{n}> (want {want})")
        })
        .collect();
    wrong.dedup();
    assert!(
        wrong.is_empty(),
        "{} builtin(s) answer the wrong palette preview:\n  {}\n\n{}",
        wrong.len(),
        wrong.join("\n  "),
        failures(&result)
    );
}
