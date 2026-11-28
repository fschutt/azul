//! The first layout of a macOS window builds no accessibility tree: the tree is built on demand (inside the
//! adapter's `update_if_active`, while an assistive technology is active), so the mode is set BEFORE the initial
//! layout, not in `init_accessibility` after it (the startup timeline of 2026-11-26 counted 8.6 ms of the first
//! frame for a tree nobody asked for). A window cannot be made in a unit test, so the source says it.

const SOURCE: &str = include_str!("mod.rs");

#[test]
fn the_on_demand_mode_is_set_before_the_initial_layout() {
    let initial_layout = SOURCE
        .find("Performing initial layout")
        .expect("the window's constructor lays out");
    let first_on_demand = SOURCE
        .find("lw.a11y_manager.on_demand = true;")
        .expect("the on-demand mode is set somewhere");
    assert!(
        first_on_demand < initial_layout,
        "the mode is set after the initial layout: that layout builds a tree"
    );
}

#[test]
fn a_debug_or_e2e_reader_keeps_the_eager_tree() {
    // The same guard as `init_accessibility`: a run with a reader of the layout side's tree (the debug server, an E2E
    // script) reads it without an adapter, so it must exist.
    let before_layout = &SOURCE[..SOURCE.find("Performing initial layout").expect("the layout")];
    let at = before_layout
        .rfind("lw.a11y_manager.on_demand = true;")
        .expect("set before the layout");
    let guard = &before_layout[at.saturating_sub(200)..at];
    assert!(guard.contains("a11y_eager()"), "the early mode honours a11y_eager(): {guard}");
}
