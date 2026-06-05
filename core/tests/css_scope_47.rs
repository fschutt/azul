//! Regression test for #47: inline (`set_css`/`with_css`) CSS must scope to the
//! owning node's subtree and must NOT leak to siblings or the root.
//!
//! Before the fix, `set_css("background: red")` parsed to a `* { … }` rule that
//! `restyle` treated as global-only, so a non-root node's inline css painted the
//! whole tree. The fix push_front-s a `Root(start..=end)` subtree-range selector
//! onto every inline rule at flatten time, scoping it to that node's subtree.

use azul_core::{
    dom::{Dom, NodeId},
    styled_dom::StyledDom,
};

/// Two sibling leaf divs, each with its own inline background. Tree (flat ids):
///   body[0]
///     div[1]  set_css("background: red")
///     div[2]  set_css("background: blue")
#[test]
fn inline_css_scopes_to_subtree_no_leak() {
    let dom = Dom::create_body()
        .with_child(Dom::create_div().with_css("background: red"))
        .with_child(Dom::create_div().with_css("background: blue"));
    let styled_dom = StyledDom::create_from_dom(dom);

    let cache = styled_dom.get_css_property_cache();
    let nd = styled_dom.node_data.as_container();
    let sn = styled_dom.styled_nodes.as_container();
    let bg = |i: usize| {
        let id = NodeId::new(i);
        cache
            .get_background_content(&nd[id], &id, &sn[id].styled_node_state)
            .cloned()
    };

    let div_a = bg(1);
    let div_b = bg(2);
    let body = bg(0);

    // Each leaf div gets its OWN inline background (subtree range == itself).
    assert!(div_a.is_some(), "div A should have its own background (red)");
    assert!(div_b.is_some(), "div B should have its own background (blue)");
    // No cross-leak: under the old global-leak bug both divs received the same
    // merged global background, so they'd compare equal.
    assert_ne!(
        div_a, div_b,
        "div A (red) and div B (blue) must differ — inline css must not cross-leak between siblings"
    );
    // THE #47 assertion: neither red nor blue may leak to the root/body.
    assert!(
        body.is_none(),
        "body must have NO background — inline css must not leak to the root (#47)"
    );
}

/// Node-only semantics (#47): a parent's NON-inherited property (background) must
/// NOT apply to its children. Subtree (`range.contains`) matching would wrongly
/// paint the child with the parent's background; node-only (`node==range.start`)
/// matching keeps it on the owner only. This is the case that made the red/blue
/// boxes render white (the root body's `background:white` covered everything).
#[test]
fn parent_non_inherited_prop_does_not_leak_to_child() {
    // body[0]
    //   parent div[1]  set_css("background: red")
    //     child div[2]  (no css)
    let child = Dom::create_div();
    let parent = Dom::create_div().with_css("background: red").with_child(child);
    let dom = Dom::create_body().with_child(parent);
    let styled_dom = StyledDom::create_from_dom(dom);

    let cache = styled_dom.get_css_property_cache();
    let nd = styled_dom.node_data.as_container();
    let sn = styled_dom.styled_nodes.as_container();
    let bg = |i: usize| {
        let id = NodeId::new(i);
        cache
            .get_background_content(&nd[id], &id, &sn[id].styled_node_state)
            .cloned()
    };

    assert!(bg(1).is_some(), "parent div should have its own background (red)");
    assert!(
        bg(2).is_none(),
        "child div must NOT receive the parent's background — set_css is node-only, not subtree"
    );
}
