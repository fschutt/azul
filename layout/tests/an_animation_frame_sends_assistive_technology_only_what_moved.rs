//! An animation frame sends assistive technology only what moved.
//!
//! The accessibility tree was rebuilt and published WHOLE after every layout
//! pass - every frame of a layout-property tween included: the switch knob's
//! 8 px slide re-sent all ~3500 nodes of AzWidgets to the platform adapter
//! (~3 ms per frame, measured on the wave-8 build, A11YPATCH8), and a relayout
//! that changed nothing re-sent them too. accesskit takes patches: a
//! `TreeUpdate` with `tree: None` and only the new / changed nodes (a removed
//! node leaves with its parent's new child list).
//!
//! The rules pinned here:
//! - a pass that changes nothing a screen reader can see publishes NOTHING;
//! - a frame that moves one box publishes that box, and only it;
//! - a rebuilt page publishes a patch, not the tree;
//! - whatever was published, the real `accesskit_consumer` takes it, and ends up holding exactly
//!   the tree a fresh full build of the same window makes.
//!
//! Not compiled by the author (house rule). Expected RED before the fix: the
//! first three tests (every pass parks a full tree).
#![cfg(feature = "a11y")]

use std::collections::BTreeMap;

use accesskit::{Node as A11yNode, NodeId as A11yNodeId, TreeUpdate};
use accesskit_consumer::{Tree, TreeChangeHandler};
use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::props::{layout::LayoutMarginLeft, property::CssProperty};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, managers::a11y::A11yManager, overlay::ContentChange,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const WIDTH: f32 = 800.0;
const HEIGHT: f32 = 600.0;

// ---- the page ----

/// A settings card: a heading and a paragraph.
fn card(i: usize) -> Dom {
    Dom::create_div()
        .with_css("padding: 8px;")
        .with_child(Dom::create_div_with_text(format!("Card {i}")))
        .with_child(Dom::create_p().with_child(
            Dom::create_text_do_not_use_without_block_level_wrapper(format!(
                "Paragraph {i}: lorem ipsum dolor sit amet, consectetur adipiscing elit."
            )),
        ))
}

/// A switch row (a label, a fixed-size track and its knob) above `cards`
/// cards.
fn page(cards: usize) -> Dom {
    let mut body = Dom::create_body()
        .with_css("margin: 0; display: flex; flex-direction: column;")
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; align-items: center; padding: 8px;")
                .with_child(Dom::create_div_with_text("Wi-Fi").with_css("flex-grow: 1;"))
                .with_child(
                    Dom::create_div()
                        .with_class("track".into())
                        .with_css("display: flex; width: 36px; height: 20px; padding: 2px;")
                        .with_child(
                            Dom::create_div()
                                .with_class("knob".into())
                                .with_css("width: 16px; height: 16px; margin-left: 0px;"),
                        ),
                ),
        );
    for i in 0..cards {
        body = body.with_child(card(i));
    }
    body
}

// ---- the harness ----

/// A window that laid `dom` out cold and once more warm.
fn window(mut dom: Dom) -> LayoutWindow {
    let styled_dom = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(WIDTH, HEIGHT);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled_dom,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    relayout(&mut lw);
    lw
}

/// Lay the retained page out again (the relayout entry every animation frame
/// takes).
fn relayout(lw: &mut LayoutWindow) {
    let result = lw.layout_results.remove(&DomId::ROOT_ID).expect("laid out");
    let window_state = lw.current_window_state.clone();
    lw.layout_and_generate_display_list(
        result.styled_dom,
        &window_state,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out again");
}

/// Lay the window out with a REBUILT page (a new `StyledDom`, as an app's
/// `RefreshDom` hands one over).
fn rebuild(lw: &mut LayoutWindow, mut dom: Dom) {
    let styled_dom = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let window_state = lw.current_window_state.clone();
    lw.layout_and_generate_display_list(
        styled_dom,
        &window_state,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the rebuilt page lays out");
}

/// Restyle `node` the way an animation frame does (`tick_animations`): an
/// override, plus the node staged as layout dirt, then lay the window out.
fn restyle(lw: &mut LayoutWindow, node: NodeId, prop: CssProperty) {
    let scope = prop.get_type().relayout_scope(false);
    let _ = lw.apply_content_change(ContentChange::NodeCss {
        dom_id: DomId::ROOT_ID,
        node_id: node,
        props: vec![prop],
        override_only: true,
    });
    lw.pending_css_dirty = Some((DomId::ROOT_ID, vec![(node, scope)]));
    relayout(lw);
}

fn with_class(lw: &LayoutWindow, class: &str) -> NodeId {
    let sd = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
    let node_data = sd.node_data.as_container();
    (0..node_data.len())
        .map(NodeId::new)
        .find(|n| {
            node_data[*n]
                .get_ids_and_classes()
                .iter()
                .any(|c| c.as_class() == Some(class))
        })
        .expect("the node exists")
}

/// A root-DOM node's accessibility id: `(dom << 32) | (index + 1)`.
fn a11y_id(node: NodeId) -> A11yNodeId {
    A11yNodeId(node.index() as u64 + 1)
}

fn bounds_x0(update: &TreeUpdate, id: A11yNodeId) -> Option<f64> {
    update
        .nodes
        .iter()
        .find(|(i, _)| *i == id)
        .and_then(|(_, n)| n.bounds())
        .map(|r| r.x0)
}

/// The tree a FRESH full build makes of the window as it is now - no retained
/// state, the reference every published update must add up to.
fn fresh_full_tree(lw: &LayoutWindow) -> TreeUpdate {
    A11yManager::update_tree(
        lw.a11y_manager.root_id,
        &lw.layout_results,
        &lw.scroll_manager,
        &lw.current_window_state.title,
        lw.current_window_state.size.dimensions,
        lw.focus_manager.get_focused_node().copied(),
        lw.current_window_state.size.get_hidpi_factor().inner.get(),
        &BTreeMap::new(),
        None,
    )
}

struct Noop;
impl TreeChangeHandler for Noop {
    fn node_added(&mut self, _: &accesskit_consumer::Node<'_>) {}
    fn node_updated(&mut self, _: &accesskit_consumer::Node<'_>, _: &accesskit_consumer::Node<'_>) {
    }
    fn focus_moved(
        &mut self,
        _: Option<&accesskit_consumer::Node<'_>>,
        _: Option<&accesskit_consumer::Node<'_>>,
    ) {
    }
    fn node_removed(&mut self, _: &accesskit_consumer::Node<'_>) {}
}

/// Hand whatever is parked to the REAL consumer, as a shell does. Returns
/// the update (`None`: nothing was parked).
fn deliver(lw: &mut LayoutWindow, tree: &mut Option<Tree>) -> Option<TreeUpdate> {
    let update = lw.a11y_manager.take_pending()?;
    match tree {
        None => *tree = Some(Tree::new(update.clone(), true)),
        Some(t) => t.update_and_process_changes(update.clone(), &mut Noop),
    }
    Some(update)
}

/// Every node the consumer holds, reachable from its root.
fn consumer_nodes(tree: &Tree) -> BTreeMap<u64, A11yNode> {
    let mut out = BTreeMap::new();
    let mut stack = vec![tree.state().root()];
    while let Some(node) = stack.pop() {
        out.insert(node.locate().0 .0, node.data().clone());
        stack.extend(node.children());
    }
    out
}

/// The consumer holds exactly what a fresh full build of `lw` makes.
fn assert_consumer_holds_the_fresh_tree(lw: &LayoutWindow, tree: &Tree, what: &str) {
    let fresh = fresh_full_tree(lw);
    let held = consumer_nodes(tree);
    assert_eq!(
        held.len(),
        fresh.nodes.len(),
        "{what}: the screen reader holds {} nodes, a fresh build makes {}",
        held.len(),
        fresh.nodes.len()
    );
    for (id, node) in &fresh.nodes {
        let is = held.get(&id.0);
        assert!(
            is == Some(node),
            "{what}: node {id:?} is {is:?} on the screen reader's side, a fresh build makes \
             {node:?}"
        );
    }
}

// ---- the tests ----

/// A relayout that changes nothing publishes nothing.
#[test]
fn a_relayout_that_changes_nothing_sends_assistive_technology_nothing() {
    let mut lw = window(page(6));
    let _ = lw.a11y_manager.take_pending();

    relayout(&mut lw);

    let parked = lw.a11y_manager.take_pending();
    assert!(
        parked.is_none(),
        "a relayout of an unchanged page published {} accessibility nodes (tree: {}) - the \
         screen reader re-reads a tree that did not change",
        parked.as_ref().map_or(0, |u| u.nodes.len()),
        parked.as_ref().is_some_and(|u| u.tree.is_some())
    );
}

/// A frame that moves the knob by 8 px publishes the knob - its bounds are
/// the only thing that changed - and nothing else; the frame after it, which
/// moves nothing, publishes nothing.
#[test]
fn a_frame_that_moves_one_box_sends_only_that_box() {
    let mut lw = window(page(6));
    let before = lw
        .a11y_manager
        .take_pending()
        .expect("the first layouts publish the tree");
    let knob = with_class(&lw, "knob");
    let knob_id = a11y_id(knob);
    let x0_before = bounds_x0(&before, knob_id).expect("harness: the knob has bounds");

    restyle(
        &mut lw,
        knob,
        CssProperty::const_margin_left(LayoutMarginLeft::const_px(8)),
    );

    let update = lw
        .a11y_manager
        .take_pending()
        .expect("the knob moved: a screen reader needs its new bounds");
    assert!(
        update.tree.is_none(),
        "a frame that moved one box re-sent the whole tree ({} nodes)",
        update.nodes.len()
    );
    let sent: Vec<A11yNodeId> = update.nodes.iter().map(|(id, _)| *id).collect();
    assert_eq!(
        sent,
        vec![knob_id],
        "a frame that moved only the knob must send only the knob"
    );
    let x0_after = bounds_x0(&update, knob_id).expect("the knob still has bounds");
    assert!(
        (x0_after - x0_before - 8.0).abs() < 0.5,
        "the knob's bounds move with it: {x0_before} -> {x0_after}"
    );

    relayout(&mut lw);
    assert!(
        lw.a11y_manager.take_pending().is_none(),
        "the frame after the move changed nothing and must send nothing"
    );
}

/// A page rebuilt with a card less, then with two more, publishes patches:
/// the parent whose child list changed and the new nodes - never the tree.
#[test]
fn a_rebuilt_page_sends_a_patch_not_the_whole_tree() {
    let mut lw = window(page(6));
    let full = lw
        .a11y_manager
        .take_pending()
        .expect("the first layouts publish the tree");
    assert!(
        full.tree.is_some(),
        "harness: the first update is a full tree"
    );

    rebuild(&mut lw, page(5));
    let removal = lw
        .a11y_manager
        .take_pending()
        .expect("a card left: its parent's child list changed");
    assert!(
        removal.tree.is_none() && removal.nodes.len() < full.nodes.len() / 4,
        "removing one card re-sent {} of {} nodes (tree: {})",
        removal.nodes.len(),
        full.nodes.len(),
        removal.tree.is_some()
    );

    rebuild(&mut lw, page(7));
    let addition = lw.a11y_manager.take_pending().expect("two cards arrived");
    assert!(
        addition.tree.is_none() && addition.nodes.len() < full.nodes.len() / 2,
        "adding two cards re-sent {} of {} nodes (tree: {})",
        addition.nodes.len(),
        full.nodes.len(),
        addition.tree.is_some()
    );
}

/// Whatever the passes publish - the first tree, a moved knob, a card
/// removed from the middle (every later node shifts its index), cards added -
/// the real consumer takes it without a panic and ends up holding exactly the
/// tree a fresh full build of the window makes.
#[test]
fn every_published_patch_leaves_the_screen_reader_holding_the_fresh_tree() {
    let mut lw = window(page(6));
    let mut tree = None;
    deliver(&mut lw, &mut tree).expect("the first layouts publish the tree");
    assert_consumer_holds_the_fresh_tree(&lw, tree.as_ref().unwrap(), "first tree");

    let knob = with_class(&lw, "knob");
    restyle(
        &mut lw,
        knob,
        CssProperty::const_margin_left(LayoutMarginLeft::const_px(8)),
    );
    deliver(&mut lw, &mut tree);
    assert_consumer_holds_the_fresh_tree(&lw, tree.as_ref().unwrap(), "knob moved");

    rebuild(&mut lw, page(3));
    deliver(&mut lw, &mut tree);
    assert_consumer_holds_the_fresh_tree(&lw, tree.as_ref().unwrap(), "three cards removed");

    rebuild(&mut lw, page(8));
    deliver(&mut lw, &mut tree);
    assert_consumer_holds_the_fresh_tree(&lw, tree.as_ref().unwrap(), "five cards added");

    // Two passes parked before a shell drains: the fold of both must still
    // be a patch the consumer takes.
    rebuild(&mut lw, page(2));
    rebuild(&mut lw, page(4));
    deliver(&mut lw, &mut tree);
    assert_consumer_holds_the_fresh_tree(&lw, tree.as_ref().unwrap(), "two passes folded");

    assert!(
        lw.a11y_manager.last_rejection.is_none(),
        "no pass needed a refusal, got {:?}",
        lw.a11y_manager.last_rejection
    );
}

/// What a frame costs the accessibility pass, not only what it sends: a
/// frame that changes nothing builds no node at all (every node's inputs are
/// unchanged, so the published one is reused), and a frame that moves the
/// knob builds the knob alone. The full rebuild built all of them, every
/// frame.
#[test]
fn an_animation_frame_builds_only_the_nodes_it_changed() {
    let mut lw = window(page(6));
    relayout(&mut lw);
    let idle = lw.a11y_manager.last_pass;
    assert!(
        idle.nodes > 20,
        "harness: the page has a real tree, {idle:?}"
    );
    assert_eq!(
        (idle.built, idle.sent, idle.published),
        (0, 0, false),
        "a frame that changed nothing built or sent accessibility nodes: {idle:?}"
    );

    let knob = with_class(&lw, "knob");
    restyle(
        &mut lw,
        knob,
        CssProperty::const_margin_left(LayoutMarginLeft::const_px(8)),
    );
    let moved = lw.a11y_manager.last_pass;
    assert_eq!(
        (moved.built, moved.sent, moved.full),
        (1, 1, false),
        "a frame that moved the knob must build and send the knob alone: {moved:?}"
    );
}
