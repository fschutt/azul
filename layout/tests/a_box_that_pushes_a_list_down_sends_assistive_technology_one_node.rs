//! A frame of an animation that pushes a list down sends assistive technology
//! the list, not every row of it.
//!
//! A node's bounds are in its accessible parent's space and its accesskit
//! `transform` is its offset from that parent. A box that moves changes its
//! own offset and nothing below it: AzDrive's tab content growing over the
//! folder list moves the list's container, and only that node is published.
//! With window-space bounds every row under the growing box was a changed
//! node on every frame, and the macOS adapter walked each one's ancestors in
//! the draw call (15 % of the main thread with an assistive client active).
#![cfg(feature = "a11y")]

use std::collections::{BTreeMap, BTreeSet};

use accesskit::{Node as A11yNode, NodeId as A11yNodeId, TreeUpdate};
use accesskit_consumer::{Tree, TreeChangeHandler};
use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::props::{layout::LayoutHeight, property::CssProperty};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    managers::a11y::{window_bounds_of, A11yManager},
    overlay::ContentChange,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const ROWS: usize = 40;

/// A box of 40 px over a list of rows, each a file name.
fn page() -> Dom {
    let mut list = Dom::create_div().with_class("list".into());
    for i in 0..ROWS {
        list = list.with_child(
            Dom::create_div()
                .with_class("row".into())
                .with_css("height: 20px;")
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(format!(
                    "file-{i:05}.txt"
                ))),
        );
    }
    Dom::create_body()
        .with_css("margin: 0; display: flex; flex-direction: column;")
        .with_child(
            Dom::create_div()
                .with_class("grow".into())
                .with_css("height: 40px;")
                .with_child(Dom::create_div_with_text("Home")),
        )
        .with_child(list)
}

fn window() -> LayoutWindow {
    let mut dom = page();
    let styled_dom = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(800.0, 1200.0);
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

/// Lay the retained page out again, the relayout every animation frame takes.
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

/// One animation frame of the box's height: an override plus the node
/// staged as layout dirt, then the relayout (`tick_animations`' shape).
fn grow_to(lw: &mut LayoutWindow, node: NodeId, px: isize) {
    let prop = CssProperty::const_height(LayoutHeight::const_px(px));
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

fn nodes_with_class(lw: &LayoutWindow, class: &str) -> Vec<NodeId> {
    let sd = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
    let node_data = sd.node_data.as_container();
    (0..node_data.len())
        .map(NodeId::new)
        .filter(|n| {
            node_data[*n]
                .get_ids_and_classes()
                .iter()
                .any(|c| c.as_class() == Some(class))
        })
        .collect()
}

/// A root-DOM node's accessibility id: `(dom << 32) | (index + 1)`.
fn a11y_id(node: NodeId) -> A11yNodeId {
    A11yNodeId(node.index() as u64 + 1)
}

/// The node and every node of its DOM subtree, as accessibility ids.
fn subtree_ids(lw: &LayoutWindow, root: NodeId) -> BTreeSet<A11yNodeId> {
    let sd = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
    let hierarchy = sd.node_hierarchy.as_container();
    let end = root.index() + hierarchy.subtree_len(root) + 1;
    (root.index()..end).map(NodeId::new).map(a11y_id).collect()
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

/// Hand whatever is parked to the real consumer, as a shell does.
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

fn fresh_full_tree(lw: &LayoutWindow) -> TreeUpdate {
    let language = lw.active_language();
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
        language.id.as_str(),
        language.is_rtl,
    )
}

/// Every frame of a box growing from 40 to 100 px over the list sends the
/// box and the list's container - never a row - and the screen reader's tree
/// stays the one a fresh build makes, with the rows drawn where they are.
#[test]
fn a_box_that_pushes_a_list_down_sends_the_list_not_its_rows() {
    let mut lw = window();
    let mut tree = None;
    deliver(&mut lw, &mut tree).expect("the first layouts publish the tree");
    let grow = nodes_with_class(&lw, "grow")[0];
    let list = nodes_with_class(&lw, "list")[0];
    let rows = nodes_with_class(&lw, "row");
    assert_eq!(rows.len(), ROWS, "harness: the list has its rows");
    let list_subtree = subtree_ids(&lw, list);
    let first_row_y0 = |lw: &LayoutWindow| {
        window_bounds_of(&lw.a11y_manager.full_tree(), a11y_id(rows[0]))
            .expect("the first row has bounds")
            .y0
    };
    let y0_start = first_row_y0(&lw);

    for (frame, px) in (45..=100).step_by(5).enumerate() {
        grow_to(&mut lw, grow, px);
        let update = deliver(&mut lw, &mut tree).expect("the frame moved the list");
        let sent: Vec<A11yNodeId> = update.nodes.iter().map(|(id, _)| *id).collect();
        let rows_sent: Vec<&A11yNodeId> = sent
            .iter()
            .filter(|id| list_subtree.contains(id) && **id != a11y_id(list))
            .collect();
        assert!(
            rows_sent.is_empty(),
            "frame {frame} (box {px} px) sent {} node(s) inside the list that did not move in it \
             ({} sent in all): every row is placed in the list's space",
            rows_sent.len(),
            sent.len()
        );
        assert!(
            sent.len() <= 3,
            "frame {frame} (box {px} px) sent {} nodes; the box and the list moved: {sent:?}",
            sent.len()
        );
        assert!(
            sent.contains(&a11y_id(list)),
            "frame {frame}: the list moved and was not sent"
        );
        let held = consumer_nodes(tree.as_ref().expect("delivered"));
        let fresh = fresh_full_tree(&lw);
        for (id, node) in &fresh.nodes {
            assert!(
                held.get(&id.0) == Some(node),
                "frame {frame}: node {id:?} differs between the screen reader and a fresh build"
            );
        }
        #[allow(clippy::cast_precision_loss)] // a small pixel count
        let moved = f64::from(px as f32 - 40.0);
        let y0 = first_row_y0(&lw);
        assert!(
            (y0 - y0_start - moved).abs() < 0.5,
            "frame {frame}: the first row is drawn at y {y0}, {moved} px below {y0_start} expected"
        );
    }
}
