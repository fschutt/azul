//! `mounted_nodes`: which node of a mounted document each numeric id is,
//! without running the document.
//!
//! `gen-e2e`'s gate resolves a scenario's `node_id` with it, so the table has to
//! be the REAL one: the same document construction as the `mount` op, which is
//! pinned below by driving the engine with the ids the table hands out.

use super::{
    full::{mounted_nodes, MountedKind, TextLines},
    run_e2e_test, E2eTest,
};

fn lines(src: &[&str]) -> TextLines {
    TextLines::Lines(src.iter().map(|l| (*l).to_string()).collect())
}

#[test]
fn a_fragment_that_starts_with_body_has_countable_ids() {
    let nodes = mounted_nodes(
        &lines(&["<body><div id=\"root\"><p id=\"t\" class=\"a b\">hi</p><img id=\"pic\" /></div></body>"]),
        &TextLines::Empty,
    )
    .expect("the fragment parses");
    let kinds: Vec<MountedKind> = nodes.iter().map(|n| n.kind).collect();
    assert_eq!(
        kinds,
        [
            MountedKind::Html,
            MountedKind::Body,
            MountedKind::Element, // #root
            MountedKind::Element, // #t
            MountedKind::Text,    // "hi"
            MountedKind::Image,   // #pic
        ],
        "{nodes:#?}"
    );
    assert_eq!(nodes[0].tag, "html");
    assert_eq!(nodes[1].tag, "body");
    assert_eq!(nodes[2].tag, "div");
    assert_eq!(nodes[2].css_id.as_deref(), Some("root"));
    assert_eq!(nodes[3].tag, "p");
    assert_eq!(nodes[3].classes, ["a", "b"]);
    assert_eq!(nodes[4].text, "hi");
    assert_eq!(nodes[4].tag, "");
    assert_eq!(nodes[5].tag, "img");
    // The tree: html > body > #root > (#t > "hi", #pic).
    let parents: Vec<Option<usize>> = nodes.iter().map(|n| n.parent).collect();
    assert_eq!(
        parents,
        [None, Some(0), Some(1), Some(2), Some(3), Some(2)],
        "{nodes:#?}"
    );
}

#[test]
fn an_indented_fragment_has_a_whitespace_node_between_every_two_tags() {
    // What a model writes: `mount` wraps it in <html><body>, and every newline
    // and indent becomes a text node that takes an id. The first indented
    // element is NOT node 1 or 2 but 3, and its child is 5.
    let nodes = mounted_nodes(
        &lines(&["<div id=\"root\">", "  <img id=\"pic\" />", "</div>"]),
        &TextLines::Empty,
    )
    .expect("the fragment parses");
    let kinds: Vec<MountedKind> = nodes.iter().map(|n| n.kind).collect();
    assert_eq!(
        kinds,
        [
            MountedKind::Html,
            MountedKind::Body,
            MountedKind::Whitespace, // the newline after <body>
            MountedKind::Element,    // #root
            MountedKind::Whitespace, // "\n  "
            MountedKind::Image,      // #pic
            MountedKind::Whitespace, // "\n" before </div>
            MountedKind::Whitespace, // "\n" before </body>
        ],
        "{nodes:#?}"
    );
    assert_eq!(nodes[3].css_id.as_deref(), Some("root"));
    assert_eq!(nodes[5].css_id.as_deref(), Some("pic"));
    assert!(nodes[4].text.contains("  "), "{:?}", nodes[4].text);
}

#[test]
fn a_fragment_that_does_not_parse_says_what_mount_says() {
    let e = mounted_nodes(&lines(&["<div id=\"a>text</div>"]), &TextLines::Empty)
        .expect_err("an attribute value that never ends is not a document");
    assert!(e.starts_with("mount: XML parse error"), "{e}");
}

/// The table is the engine's: every element it names by id is the element the
/// engine addresses by that numeric id.
#[test]
fn the_ids_of_the_table_are_the_ids_the_engine_addresses() {
    let html = [
        "<div id=\"root\">",
        "  <div id=\"a\" class=\"box\"></div>",
        "  <p id=\"t\">text <b id=\"bold\">b</b> tail</p>",
        "  <div id=\"c\"></div>",
        "</div>",
    ];
    let nodes = mounted_nodes(&lines(&html), &TextLines::Empty).expect("parses");

    let mut steps = vec![
        serde_json::json!({ "op": "mount", "html": html }),
        serde_json::json!({ "op": "wait_frame" }),
    ];
    let mut named = 0;
    for (id, node) in nodes.iter().enumerate() {
        let Some(css_id) = &node.css_id else { continue };
        named += 1;
        steps.push(serde_json::json!({
            "op": "set_node_css_override", "node_id": id, "property": "width", "value": "77px"
        }));
        steps.push(serde_json::json!({ "op": "wait_frame" }));
        steps.push(serde_json::json!({
            "op": "get_node_css_properties", "selector": format!("#{css_id}")
        }));
        steps.push(serde_json::json!({ "op": "assert_response", "contains": "width: 77px" }));
    }
    assert_eq!(named, 5, "root, a, t, bold, c");

    let test: E2eTest = serde_json::from_value(serde_json::json!({
        "name": "mounted_nodes_are_the_engines",
        "setup": { "window_width": 400, "window_height": 300, "dpi": 96 },
        "steps": steps,
    }))
    .expect("a valid scenario");
    let result = run_e2e_test(&test);
    let red: Vec<String> = result
        .steps
        .iter()
        .filter(|s| s.status != "pass")
        .map(|s| format!("step {} {}: {:?}", s.step_index, s.op, s.error))
        .collect();
    assert!(red.is_empty(), "{}", red.join("\n"));
}
