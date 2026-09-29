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
