//! The classes a callback gives a node are what a stylesheet selects it by: the
//! rules of the class it has now apply, those of the class it lost do not
//! (`LayoutWindow::set_node_ids_and_classes`, the one function behind
//! `CallbackInfo::set_node_ids_and_classes` and the `set_node_classes` op).
//!
//! Both arms of `CallbackChange::SetNodeIdsAndClasses` (the shells' and the
//! headless runner's) used to write the names and nothing else. The layout ran,
//! the cascade did not: the node kept the styles of its old class, a
//! colour-only class swap was a dead control (`gen-e2e` batch 3, probes
//! `s1`-`s4`), and `layout_passes` read 1 for it.
//!
//! The pages carry their sheet the way a widget does, as a scoped `with_css`
//! on the node that owns the subtree (the scope is `Root([owner, end])`, which
//! a retained-sheet restyle must still honour), and a window's layout is read
//! again after the swap as the shell's incremental relayout reads it.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, IdOrClassVec, NodeId},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::props::basic::color::ColorU;
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::getters::get_background_color,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const RED: ColorU = ColorU::new_rgb(255, 0, 0);
const BLUE: ColorU = ColorU::new_rgb(0, 0, 255);
const GREEN: ColorU = ColorU::new_rgb(0, 255, 0);

/// The node order of every page here: the body is 0, the node that owns the
/// sheet 1, the node under test 2.
const OWNER: usize = 1;
const TARGET: usize = 2;

fn classes(names: &[&str]) -> IdOrClassVec {
    names
        .iter()
        .map(|name| IdOrClass::Class((*name).into()))
        .collect::<Vec<_>>()
        .into()
}

/// `body > owner(sheet, owner_classes) > target(target_classes)`. An empty
/// `sheet` leaves the page with no author rule at all (not even a bare
/// declaration: a `with_css` anywhere is a rule of the retained sheet).
fn page(sheet: &str, owner_classes: &[&str], target_classes: &[&str]) -> Dom {
    let mut owner = Dom::create_div().with_ids_and_classes(classes(owner_classes));
    if !sheet.is_empty() {
        owner = owner.with_css(sheet);
    }
    Dom::create_body().with_child(
        owner.with_child(Dom::create_div().with_ids_and_classes(classes(target_classes))),
    )
}

fn window(dom: Dom) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut dom = dom;
    let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    lw
}

/// The layout pass the shells' `ShouldIncrementalRelayout` runs: the existing
/// `StyledDom`, laid out again.
fn relayout(lw: &mut LayoutWindow) {
    let result = lw.layout_results.remove(&DomId::ROOT_ID).expect("laid out");
    let ws = lw.current_window_state.clone();
    lw.layout_and_generate_display_list(
        result.styled_dom,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out again");
}

fn swap(lw: &mut LayoutWindow, node: usize, names: &[&str]) -> bool {
    lw.set_node_ids_and_classes(DomId::ROOT_ID, NodeId::new(node), &classes(names))
}

fn background(lw: &LayoutWindow, node: usize) -> ColorU {
    let styled = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
    let id = NodeId::new(node);
    get_background_color(
        styled,
        id,
        &styled.styled_nodes.as_container()[id].styled_node_state,
    )
}

fn rect(lw: &LayoutWindow, node: usize) -> LogicalRect {
    lw.get_node_layout_rect(DomNodeId {
        dom: DomId::ROOT_ID,
        node: Some(NodeId::new(node)).into(),
    })
    .expect("laid out")
}

fn epoch(lw: &LayoutWindow) -> u64 {
    lw.layout_results[&DomId::ROOT_ID]
        .styled_dom
        .get_css_property_cache()
        .cascade_epoch
}

const COLOURS: &str = ".r { background-color: #ff0000; } .b { background-color: #0000ff; }";

/// R-classes: a colour-only swap. The old arm left the node red.
#[test]
fn r_classes_a_swapped_class_paints_with_the_rule_of_the_new_class() {
    let mut lw = window(page(COLOURS, &[], &["r"]));
    assert_eq!(background(&lw, TARGET), RED, "harness: the page starts red");
    assert!(swap(&mut lw, TARGET, &["b"]), "a new list is a change");
    relayout(&mut lw);
    assert_eq!(
        background(&lw, TARGET),
        BLUE,
        "the rule of the class the node has now applies, the old class's no longer does"
    );
}

/// R-classes: a class that is dropped takes its rule with it.
#[test]
fn r_classes_dropping_the_only_class_drops_its_rule() {
    let mut lw = window(page(COLOURS, &[], &["r"]));
    assert_eq!(background(&lw, TARGET), RED, "harness: the page starts red");
    assert!(swap(&mut lw, TARGET, &[]));
    relayout(&mut lw);
    assert_eq!(
        background(&lw, TARGET),
        ColorU::TRANSPARENT,
        "a node without the class is not matched by its rule"
    );
}

/// R-classes: the new class moves the box, not only the paint.
#[test]
fn r_classes_a_swapped_class_resizes_the_node() {
    let mut lw = window(page(
        ".n { width: 100px; height: 40px; } .w { width: 300px; height: 40px; }",
        &[],
        &["n"],
    ));
    assert!(
        (rect(&lw, TARGET).size.width - 100.0).abs() < 0.01,
        "harness: 100 wide first"
    );
    assert!(swap(&mut lw, TARGET, &["w"]));
    relayout(&mut lw);
    let after = rect(&lw, TARGET);
    assert!(
        (after.size.width - 300.0).abs() < 0.01,
        "the rule of the new class sizes the node: {after:?}"
    );
}

/// R-classes: a class on the node ABOVE changes the boxes BELOW it. The rows'
/// own data did not change, so a warm reconcile would hand them their old
/// boxes; the swap lays the window out cold (a sheet is there to match).
#[test]
fn r_classes_a_class_on_an_ancestor_resizes_the_nodes_below_it() {
    let mut lw = window(page(
        ".light > div { height: 20px; } .dark > div { height: 50px; }",
        &["light"],
        &["row"],
    ));
    assert!(
        (rect(&lw, TARGET).size.height - 20.0).abs() < 0.01,
        "harness: 20 tall first"
    );
    assert!(swap(&mut lw, OWNER, &["dark"]));
    relayout(&mut lw);
    let after = rect(&lw, TARGET);
    assert!(
        (after.size.height - 50.0).abs() < 0.01,
        "the row follows the class of its parent through the descendant rule: {after:?}"
    );
}

/// R-classes: the state rules of the new class hold by the state the node is
/// in. A hovered node takes the `:hover` rule of the class it gets.
#[test]
fn r_classes_the_hover_rule_of_the_new_class_applies_to_a_hovered_node() {
    let sheet = format!("{COLOURS} .b:hover {{ background-color: #00ff00; }}");
    let mut lw = window(page(&sheet, &[], &["r"]));
    lw.layout_results
        .get_mut(&DomId::ROOT_ID)
        .expect("laid out")
        .styled_dom
        .styled_nodes
        .as_container_mut()[NodeId::new(TARGET)]
    .styled_node_state
    .hover = true;
    assert_eq!(
        background(&lw, TARGET),
        RED,
        "harness: hovered, `.r` has no hover rule"
    );
    assert!(swap(&mut lw, TARGET, &["b"]));
    relayout(&mut lw);
    assert_eq!(
        background(&lw, TARGET),
        GREEN,
        "the node is still hovered and its class has a hover rule now"
    );
    // The state is the node's, not the class's: leaving it, the resting rule.
    lw.layout_results
        .get_mut(&DomId::ROOT_ID)
        .expect("laid out")
        .styled_dom
        .styled_nodes
        .as_container_mut()[NodeId::new(TARGET)]
    .styled_node_state
    .hover = false;
    assert_eq!(
        background(&lw, TARGET),
        BLUE,
        "unhovered, the resting rule of `.b`"
    );
}

/// R-classes: a user override (`set_css_property`) outranks the cascade, also
/// after the cascade has been re-run for a swap. A guard: it holds before the
/// fix as well, where the cascade never moved.
#[test]
fn r_classes_a_user_override_still_outranks_the_class_after_a_swap() {
    let mut lw = window(page(COLOURS, &[], &["r"]));
    let _ = lw.apply_content_change(azul_layout::overlay::ContentChange::NodeCss {
        dom_id: DomId::ROOT_ID,
        node_id: NodeId::new(TARGET),
        props: vec![azul_css::props::property::CssProperty::background_color(
            azul_css::props::style::StyleBackgroundContent::Color(GREEN),
        )],
        override_only: true,
    });
    assert_eq!(
        background(&lw, TARGET),
        GREEN,
        "harness: the override paints"
    );
    assert!(swap(&mut lw, TARGET, &["b"]));
    relayout(&mut lw);
    assert_eq!(
        background(&lw, TARGET),
        GREEN,
        "the override is above the new class too"
    );
}

/// The classes a node already has are no change: no restyle (the cascade
/// generation stays), no cold layout (the tree stays), and the caller owes
/// nothing. The combobox rewrites the classes of EVERY option on each key.
#[test]
fn r_classes_the_classes_a_node_already_has_change_nothing() {
    let mut lw = window(page(COLOURS, &[], &["r"]));
    let epoch_before = epoch(&lw);
    assert!(
        lw.layout_cache.tree.is_some(),
        "harness: a laid-out window keeps its tree"
    );
    assert!(!swap(&mut lw, TARGET, &["r"]), "the same list is no change");
    assert_eq!(
        epoch(&lw),
        epoch_before,
        "no restyle for a list the node has"
    );
    assert!(
        lw.layout_cache.tree.is_some(),
        "no cold layout for a list the node has: the tree stays"
    );
    // A different list is a change, and costs the restyle and the cold layout.
    assert!(swap(&mut lw, TARGET, &["b"]));
    assert_ne!(epoch(&lw), epoch_before, "a new list re-runs the cascade");
    assert!(
        lw.layout_cache.tree.is_none(),
        "and lays the window out cold"
    );
}

/// A node that names no node of the window is no change.
#[test]
fn r_classes_a_node_that_is_not_there_is_no_change() {
    let mut lw = window(page(COLOURS, &[], &["r"]));
    assert!(!swap(&mut lw, 99, &["b"]), "no node 99");
    assert!(
        !lw.set_node_ids_and_classes(DomId { inner: 7 }, NodeId::new(TARGET), &classes(&["b"])),
        "no DOM 7"
    );
}

/// With no sheet there is no rule a name can select: the names are written,
/// the cascade is not re-run and the warm layout stays (the reconcile finds
/// the new names by their hash).
#[test]
fn r_classes_a_window_without_a_sheet_keeps_its_cascade_and_its_warm_layout() {
    let mut lw = window(page("", &[], &["r"]));
    let epoch_before = epoch(&lw);
    assert!(swap(&mut lw, TARGET, &["b"]), "the names change");
    assert_eq!(epoch(&lw), epoch_before, "nothing to re-match: no restyle");
    assert!(lw.layout_cache.tree.is_some(), "the warm layout stays");
    let names = lw.layout_results[&DomId::ROOT_ID]
        .styled_dom
        .node_data
        .as_container()[NodeId::new(TARGET)]
    .get_ids_and_classes();
    assert_eq!(names, classes(&["b"]), "the node carries the new names");
}
