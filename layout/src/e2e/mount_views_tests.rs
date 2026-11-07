//! A mounted virtual view: `mount`'s `virtual_views` turn the empty element a selector
//! names into a list of N rows backed by a deterministic, windowed callback, driven
//! through the real op dispatch (`run_e2e_test`) on a headless window, and straight on a
//! parsed document (E2E-M1).
//!
//! The XML parser has no VirtualView - `<virtual-view>`, `<iframe>` and `data-virtual-*`
//! attributes mount as a plain `<div>` - so a scenario about a 5000-row list had no way
//! to reach the code under test: the view's windowing, its callback, the nodes that exist
//! while it scrolls (`gen-e2e` batch 1, lines 7, 8, 16 and 45-50 wrote a plain `div` of
//! rows or a 99,840 px spacer instead).

use azul_core::{dom::IdOrClass, styled_dom::StyledDom};

use super::{
    apply_views, rows_window, run_e2e_test, E2eTest, E2eTestResult, MountView, TextLines,
    MOUNT_VIEW_MAX_ROWS,
};

// ---------------------------------------------------------------------------
// The window of rows
// ---------------------------------------------------------------------------

/// A box of 300 px over rows of 24 px shows 12.5 rows; with the one it may start in and
/// a screen either side the view builds 28 of 5000.
#[test]
fn e2e_m1_a_view_at_the_top_builds_the_visible_rows_and_a_screen_of_more() {
    assert_eq!(rows_window(5000, 24.0, 0.0, 300.0), (0, 28));
}

/// The window follows the offset: row 100 is at 2400 px.
#[test]
fn e2e_m1_the_window_of_rows_follows_the_scroll_offset() {
    let (first, end) = rows_window(5000, 24.0, 2400.0, 300.0);
    assert!(
        first <= 100 && 100 + 13 <= end,
        "row 100 and the 13 below it: {first}..{end}"
    );
    assert_eq!((first, end), (86, 128));
    // Half a row down still shows the row it started in.
    let (first, end) = rows_window(5000, 24.0, 2412.0, 300.0);
    assert!(first <= 100 && 100 < end, "{first}..{end}");
}

/// Never more than the document has, whatever the offset: a scroll past the end shows the
/// last rows, and nothing the document does not have.
#[test]
fn e2e_m1_the_window_never_leaves_the_document() {
    assert_eq!(rows_window(3, 24.0, 0.0, 300.0), (0, 3));
    let (first, end) = rows_window(5000, 24.0, 1.0e9, 300.0);
    assert_eq!(end, 5000);
    assert!(first >= 4900, "the last rows: {first}..{end}");
    for offset in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -50.0, 0.0] {
        for height in [f32::NAN, f32::INFINITY, -1.0, 0.0, 300.0] {
            for row in [f32::NAN, 0.0, -3.0, 24.0] {
                let (first, end) = rows_window(10, row, offset, height);
                assert!(
                    first <= end && end <= 10,
                    "{offset} {height} {row}: {first}..{end}"
                );
            }
        }
    }
}

/// A view builds a few dozen nodes however long the document is.
#[test]
fn e2e_m1_the_window_is_the_same_size_at_any_depth_of_the_document() {
    for offset in [0.0, 24.0 * 1000.0, 24.0 * 4000.0, 24.0 * 4990.0] {
        let (first, end) = rows_window(5000, 24.0, offset, 300.0);
        assert!(end - first <= 42, "{offset}: {first}..{end}");
        assert!(end > first, "{offset}: something is built");
    }
}

// ---------------------------------------------------------------------------
// The view takes the place of an element
// ---------------------------------------------------------------------------

fn view(selector: &str) -> MountView {
    MountView {
        selector: selector.to_string(),
        rows: 5000,
        row_height: 24.0,
        row_html: TextLines::Single("<div class=\"row\">Row {n}</div>".to_string()),
        row_css: TextLines::Empty,
    }
}

fn document() -> StyledDom {
    crate::xml::parse_xml_to_styled_dom(
        "<html><body><div id=\"list\" class=\"tall\"></div><div id=\"other\"><p>x</p></div></body></html>",
    )
    .expect("a document")
}

/// The element named is a VirtualView afterwards and keeps its id and class (the page's
/// CSS addresses it as before); the other elements are what they were.
#[test]
fn e2e_m1_a_view_takes_the_place_of_the_empty_element_its_selector_names() {
    let mut dom = document();
    // html 0, body 1, #list 2, #other 3, its <p> 4, the text 5.
    let host = azul_core::id::NodeId::new(2);
    assert!(!dom.node_data.as_container()[host].is_virtual_view_node());
    apply_views(&mut dom, &[view("#list")]).expect("the view lands");
    let nodes = dom.node_data.as_container();
    assert!(nodes[host].is_virtual_view_node(), "#list is a view now");
    let names = nodes[host].get_ids_and_classes();
    assert!(
        names
            .as_ref()
            .iter()
            .any(|n| matches!(n, IdOrClass::Id(id) if id.as_str() == "list"))
            && names
                .as_ref()
                .iter()
                .any(|n| matches!(n, IdOrClass::Class(c) if c.as_str() == "tall")),
        "its id and class are still there: {names:?}"
    );
    assert!(
        !nodes[azul_core::id::NodeId::new(3)].is_virtual_view_node(),
        "no other element became one"
    );
}

/// A view that cannot land is refused by name, with the number of the view and what is
/// wrong - at the `mount`, not as a list that never appears.
#[test]
fn e2e_m1_a_view_that_cannot_land_is_refused_by_name() {
    let broken = |change: &dyn Fn(&mut MountView)| {
        let mut v = view("#list");
        change(&mut v);
        v
    };
    for (bad, want) in [
        (
            broken(&|v| v.selector = String::new()),
            "virtual_views[0] has an empty `selector`",
        ),
        (
            broken(&|v| v.selector = "#nope".to_string()),
            "must match exactly one element of the mounted document, it matches 0",
        ),
        (broken(&|v| v.selector = "div".to_string()), "it matches 2"),
        (
            broken(&|v| v.selector = "#other".to_string()),
            "the element has content",
        ),
        (broken(&|v| v.rows = 0), "rows is 0"),
        (
            broken(&|v| v.rows = MOUNT_VIEW_MAX_ROWS + 1),
            "which is not a whole number from 1 to 10000000",
        ),
        (broken(&|v| v.row_height = 0.0), "row_height is 0"),
        (broken(&|v| v.row_height = -4.0), "row_height is -4"),
        (broken(&|v| v.row_height = f32::NAN), "row_height is NaN"),
        (
            broken(&|v| v.row_html = TextLines::Empty),
            "`row_html` is empty",
        ),
        (
            broken(&|v| v.row_html = TextLines::Single("<div>".to_string())),
            "`row_html` is not valid markup",
        ),
    ] {
        let error = apply_views(&mut document(), &[bad.clone()])
            .expect_err(&format!("{bad:?} cannot land"));
        assert!(
            error.contains("virtual_views[0]") && error.contains(want),
            "wanted {want:?}, got: {error}"
        );
    }
}

/// The second view is the one that is refused, and the message numbers it.
#[test]
fn e2e_m1_the_refusal_numbers_the_view_that_is_wrong() {
    let mut second = view("#nope");
    second.rows = 10;
    let error = apply_views(&mut document(), &[view("#list"), second]).expect_err("the second");
    assert!(error.contains("virtual_views[1] ('#nope')"), "{error}");
}

// ---------------------------------------------------------------------------
// Through the mount op
// ---------------------------------------------------------------------------

/// Run `steps` as one headless scenario on a 400x300 window. A red step does not end the
/// run, so every assertion is read.
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

/// The first number named `key` anywhere in `value`.
fn find_number(value: &serde_json::Value, key: &str) -> Option<f64> {
    match value {
        serde_json::Value::Object(map) => map
            .get(key)
            .and_then(serde_json::Value::as_f64)
            .or_else(|| map.values().find_map(|v| find_number(v, key))),
        serde_json::Value::Array(items) => items.iter().find_map(|v| find_number(v, key)),
        _ => None,
    }
}

/// A 300 x 200 list of 5000 rows of 24 px, mounted with `collapse_whitespace` so the host
/// is node 2 (html 0, body 1).
fn mount_the_list() -> serde_json::Value {
    serde_json::json!({
        "op": "mount",
        "collapse_whitespace": true,
        "html": ["<div id=\"list\"></div>"],
        "css": ["html, body { margin: 0; }", "#list { width: 300px; height: 200px; overflow: auto; }"],
        "virtual_views": [{
            "selector": "#list",
            "rows": 5000,
            "row_height": 24,
            "row_html": ["<div class=\"label\">Row {n}</div>"],
            "row_css": [".label { font-size: 14px; color: black; }"],
        }],
    })
}

/// The list is a VirtualView over a document of 5000 x 24 = 120000 px, and the DOM it
/// builds holds a window of rows, not the 5000 (a few dozen nodes, row 0 among them).
#[test]
fn e2e_m1_a_mounted_5000_row_list_builds_a_window_of_rows_not_the_document() {
    let result = run(
        "e2e_m1_window",
        vec![
            mount_the_list(),
            serde_json::json!({ "op": "wait_frame" }),
            serde_json::json!({ "op": "wait", "ms": 100 }),
            serde_json::json!({ "op": "get_virtual_view_states" }),
            serde_json::json!({
                "op": "assert_response", "contains": "\"virtual_scroll_size_height\":120000.0",
            }),
            serde_json::json!({ "op": "assert_response", "contains": "\"was_invoked\":true" }),
            serde_json::json!({ "op": "get_virtual_view_layout", "node_id": 2 }),
            serde_json::json!({ "op": "assert_response", "contains": "\"id\":\"row-0\"" }),
            serde_json::json!({ "op": "assert_response", "not_contains": "\"id\":\"row-4999\"" }),
        ],
    );
    assert_passes(&result);
    let layout = result
        .steps
        .iter()
        .find(|s| s.op == "get_virtual_view_layout")
        .and_then(|s| s.response.as_ref())
        .expect("the layout of the view");
    let nodes = find_number(layout, "node_count").expect("the response counts the nodes");
    assert!(
        (20.0..300.0).contains(&nodes),
        "a window of rows (about 20 of 5000, three nodes each), not the document: {nodes} nodes"
    );
}

/// Scrolling the list builds the rows it scrolled to and drops the ones it left: the view is
/// asked again as the window nears the edge of what it built.
#[test]
fn e2e_m1_scrolling_the_list_builds_the_rows_it_scrolled_to() {
    let result = run(
        "e2e_m1_scroll",
        vec![
            mount_the_list(),
            serde_json::json!({ "op": "wait_frame" }),
            serde_json::json!({ "op": "wait", "ms": 100 }),
            serde_json::json!({
                "op": "scroll_node_by", "selector": "#list", "delta_x": 0, "delta_y": 2400,
            }),
            serde_json::json!({ "op": "wait_frame" }),
            serde_json::json!({ "op": "tick_ms", "ms": 250 }),
            serde_json::json!({ "op": "wait_frame" }),
            serde_json::json!({ "op": "get_virtual_view_layout", "node_id": 2 }),
            serde_json::json!({ "op": "assert_response", "contains": "\"id\":\"row-100\"" }),
            serde_json::json!({ "op": "assert_response", "not_contains": "\"id\":\"row-0\"" }),
        ],
    );
    assert_passes(&result);
}

/// A view that cannot land fails the `mount` step with the reason - the scenario does not run
/// on against a list that is not there.
#[test]
fn e2e_m1_a_mount_whose_view_cannot_land_fails_at_the_mount() {
    let mut mount = mount_the_list();
    mount["virtual_views"][0]["selector"] = serde_json::json!("#missing");
    let result = run("e2e_m1_refused", vec![mount]);
    let step = result
        .steps
        .iter()
        .find(|s| s.op == "mount")
        .expect("the mount step ran");
    assert_eq!(step.status, "fail", "{}", failures(&result));
    let error = step.error.clone().unwrap_or_default();
    assert!(
        error.contains("virtual_views[0] ('#missing')")
            && error.contains("must match exactly one element of the mounted document"),
        "{error}"
    );
}

/// A field a view does not have is refused by name (the typed read), not dropped.
#[test]
fn e2e_m1_a_field_a_view_does_not_have_is_refused() {
    let mut mount = mount_the_list();
    mount["virtual_views"][0]["row_count"] = serde_json::json!(5);
    let result = run("e2e_m1_typo", vec![mount]);
    let step = result
        .steps
        .iter()
        .find(|s| s.op == "mount")
        .expect("the mount step ran");
    assert_eq!(step.status, "fail", "{}", failures(&result));
    assert!(
        step.error.clone().unwrap_or_default().contains("row_count"),
        "the refusal names the field: {:?}",
        step.error
    );
}

/// The views are kept with the window that mounted them and put into every document the
/// host parses from the mounted XML ([`apply_mount_views`]); a window with none leaves the
/// document as it parsed, and the next `mount` (which sets the list afresh, `unmount` an
/// empty one) replaces them.
#[test]
fn e2e_m1_the_views_are_kept_with_the_window_and_put_into_the_document_it_parses() {
    use super::{apply_mount_views, e2e_mount_views, e2e_set_mount_views};

    let lw = crate::window::LayoutWindow::new(rust_fontconfig::FcFontCache::default())
        .expect("a layout window");
    let host = azul_core::id::NodeId::new(2);

    assert!(e2e_mount_views(&lw).is_empty(), "a fresh window has none");
    let mut untouched = document();
    apply_mount_views(&lw, &mut untouched);
    assert!(
        !untouched.node_data.as_container()[host].is_virtual_view_node(),
        "no views: the document is what the host parsed"
    );

    e2e_set_mount_views(&lw, vec![view("#list")]);
    assert_eq!(e2e_mount_views(&lw).len(), 1);
    // Twice: a re-parse after a theme switch gets them too.
    for _ in 0..2 {
        let mut parsed = document();
        apply_mount_views(&lw, &mut parsed);
        assert!(parsed.node_data.as_container()[host].is_virtual_view_node());
    }

    e2e_set_mount_views(&lw, Vec::new());
    let mut next = document();
    apply_mount_views(&lw, &mut next);
    assert!(
        !next.node_data.as_container()[host].is_virtual_view_node(),
        "the next mount asked for none"
    );
}
