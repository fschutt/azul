//! The graph box of a node graph, laid out in a page: what the box the nodes
//! stand in is as tall as, and so whether the graph shows its nodes at all.
//!
//! Every node and every wire of the graph is absolutely positioned inside the
//! graph box (`.nodegraph`, `overflow: hidden`), so the nodes give the box no
//! height. It takes the height of the wrapper around it: `.nodegraph`
//! declares `flex-grow: 1` and no size, and `flex-grow` acts in a flex
//! container only. The wrapper was a block container, the graph box in it was
//! 0 px tall, and its `overflow: hidden` clipped every node: the map editor's
//! panel of the graph was empty. The wrapper is a flex container.
//!
//! The page is the editor's in small (`render_main_content` of mapedit3): a
//! column of a bar and a row, the row of a side bar and the graph, in a window
//! of 800 x 600 px. The graph gets no size of its own, only what the row gives
//! its wrapper: 750 x 560 px at (50, 40). The tests of `audit_a3_solver_tests`
//! size the graph box by a stylesheet, which is why none of them saw it
//! collapse. The window has no fonts, so no size that depends on one is
//! asserted (a node box is 272 x 82 px here, and taller with text). No wire is
//! asserted either: `draw_connection` draws nothing yet (the widget's
//! documented limitation), so a wire has no pixels to show.

use azul_core::{
    dom::{DomId, DomNodeId, NodeId},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use rust_fontconfig::FcFontCache;

use super::*;
use crate::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};

/// The window the page is laid out in, px.
const WINDOW_W: f32 = 800.0;
const WINDOW_H: f32 = 600.0;

/// Where the three nodes of the graph stand in the graph. A node box is about
/// 270 x 80 px in a window without fonts (taller with them), so all three are
/// inside a graph box of 750 x 560 px.
static AT: [(f32, f32); 3] = [(20.0, 30.0), (320.0, 60.0), (120.0, 260.0)];

/// The two ways the graph is built into DOM: all of it, and culled to the
/// window (`dom_in_view`, which the editor builds it with).
static BUILDS: [&str; 2] = ["dom", "dom_in_view"];

// ---- the graph ----

/// A graph of one node type (one input and one output of one port type, no
/// field) with a node at each of `at`, ids from 1.
fn graph_of(at: &[(f32, f32)]) -> NodeGraph {
    let kind = NodeTypeId { inner: 1 };
    let port = InputOutputTypeId { inner: 10 };
    let mut g = NodeGraph {
        node_types: vec![NodeTypeIdInfoMap {
            node_type_id: kind,
            node_type_info: NodeTypeInfo {
                is_root: true,
                node_type_name: AzString::from_const_str("kind"),
                inputs: vec![port].into(),
                outputs: vec![port].into(),
            },
        }]
        .into(),
        input_output_types: vec![InputOutputTypeIdInfoMap {
            io_type_id: port,
            io_info: InputOutputInfo {
                data_type: AzString::from_const_str("port"),
                color: ColorU {
                    r: 1,
                    g: 2,
                    b: 3,
                    a: 255,
                },
            },
        }]
        .into(),
        ..NodeGraph::default()
    };
    for (id, (x, y)) in (1_u64..).zip(at.iter()) {
        g.nodes.push(NodeIdNodeMap {
            node_id: NodeGraphNodeId { inner: id },
            node: NodeGraphNode {
                node_type: kind,
                position: NodeGraphNodePosition { x: *x, y: *y },
                fields: NodeTypeFieldVec::new(),
                connect_in: InputConnectionVec::new(),
                connect_out: OutputConnectionVec::new(),
            },
        });
    }
    g
}

/// The three nodes of [`AT`], the output of node 1 wired to the input of node 2.
fn three_nodes() -> NodeGraph {
    let mut g = graph_of(&AT);
    g.connect_input_output(
        NodeGraphNodeId { inner: 2 },
        0,
        NodeGraphNodeId { inner: 1 },
        0,
    )
    .expect("a node feeds another");
    g
}

/// `graph` as DOM, built the way `how` says ([`BUILDS`]).
fn built(graph: NodeGraph, how: &str) -> Dom {
    if how == "dom" {
        graph.dom()
    } else {
        graph.dom_in_view(LogicalSize::new(WINDOW_W, WINDOW_H))
    }
}

// ---- the page ----

/// The editor's page in small, `graph` its second box of the row: a column of a
/// 40 px bar and the editor row, the row of a 50 px side bar and the graph.
fn page_of(graph: Dom) -> Dom {
    Dom::create_body()
        .with_css(
            "margin: 0px; display: flex; flex-direction: column; width: 800px; height: 600px;",
        )
        .with_children(
            vec![
                Dom::create_div().with_css("height: 40px; flex-shrink: 0;"),
                Dom::create_div()
                    .with_ids_and_classes(
                        vec![IdOrClass::Id(AzString::from_const_str("editor"))].into(),
                    )
                    .with_css("display: flex; flex-direction: row; flex-grow: 1; min-height: 0;")
                    .with_children(
                        vec![
                            Dom::create_div().with_css("width: 50px; flex-shrink: 0;"),
                            graph,
                        ]
                        .into(),
                    ),
            ]
            .into(),
        )
}

/// `page` styled and laid out in the window, no font in it.
fn laid_out(page: Dom) -> LayoutWindow {
    let styled = StyledDom::create_from_dom(page);
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(WINDOW_W, WINDOW_H);
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

/// The page with `graph` built as `how` says, laid out.
fn window_of(graph: NodeGraph, how: &str) -> LayoutWindow {
    laid_out(page_of(built(graph, how)))
}

// ---- reading the laid-out page ----

/// The nodes of the page that carry the class `class`, in document order.
fn nodes_with_class(lw: &LayoutWindow, class: &str) -> Vec<NodeId> {
    let nodes = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("laid out")
        .styled_dom
        .node_data
        .as_container();
    (0..nodes.len())
        .map(NodeId::new)
        .filter(|n| nodes[*n].has_class(class))
        .collect()
}

/// The node of the page with the id `id`.
fn node_with_id(lw: &LayoutWindow, id: &str) -> NodeId {
    let nodes = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("laid out")
        .styled_dom
        .node_data
        .as_container();
    (0..nodes.len())
        .map(NodeId::new)
        .find(|n| nodes[*n].has_id(id))
        .unwrap_or_else(|| panic!("harness: no node with the id {id}"))
}

/// The graph box: the one node of the page with the class `nodegraph`.
fn graph_box_of(lw: &LayoutWindow) -> NodeId {
    let found = nodes_with_class(lw, "nodegraph");
    assert_eq!(found.len(), 1, "harness: one graph box in the page");
    found[0]
}

/// The parent of `node`; the wrapper of the graph, for the graph box.
fn parent_of(lw: &LayoutWindow, node: NodeId) -> NodeId {
    let hierarchy = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("laid out")
        .styled_dom
        .node_hierarchy
        .as_container();
    hierarchy[node]
        .parent_id()
        .unwrap_or_else(|| panic!("harness: {node:?} has no parent"))
}

fn dom_node(node: NodeId) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(node)),
    }
}

/// Where the layout put `node` and how big: its border box in window px,
/// before any `transform`.
fn rect_of(lw: &LayoutWindow, node: NodeId) -> LogicalRect {
    let id = dom_node(node);
    let size = lw
        .get_node_size(id)
        .unwrap_or_else(|| panic!("harness: {node:?} has no size"));
    let origin = lw
        .get_node_position(id)
        .unwrap_or_else(|| panic!("harness: {node:?} has no position"));
    LogicalRect::new(origin, size)
}

/// Where `node` is shown in the window: its box through the `transform`s of
/// itself and of its ancestors (the engine's own answer, the one a context menu
/// and a synthetic click are placed with).
fn on_screen(lw: &LayoutWindow, node: NodeId) -> Option<LogicalRect> {
    lw.get_node_hit_test_bounds(dom_node(node))
}

/// The boxes the tests read, for the end of a message: the editor row, the
/// wrapper of the graph, the graph box and the node boxes (size @ origin).
fn boxes(lw: &LayoutWindow) -> String {
    let graph_box = graph_box_of(lw);
    let nodes: Vec<LogicalRect> = nodes_with_class(lw, "node_graph_node")
        .into_iter()
        .map(|n| rect_of(lw, n))
        .collect();
    format!(
        "editor row {:?}; wrapper {:?}; graph box {:?}; node boxes {:?}",
        rect_of(lw, node_with_id(lw, "editor")),
        rect_of(lw, parent_of(lw, graph_box)),
        rect_of(lw, graph_box),
        nodes,
    )
}

// ---- comparing rects ----

/// `a` and `b` within `tolerance` px.
fn within(a: f32, b: f32, tolerance: f32) -> bool {
    (a - b).abs() <= tolerance
}

/// `size` px at `(x, y)`.
fn rect(x: f32, y: f32, width: f32, height: f32) -> LogicalRect {
    LogicalRect::new(LogicalPosition::new(x, y), LogicalSize::new(width, height))
}

/// `a` and `b` are one rect, to half a px.
fn same_rect(a: &LogicalRect, b: &LogicalRect) -> bool {
    within(a.origin.x, b.origin.x, 0.5)
        && within(a.origin.y, b.origin.y, 0.5)
        && within(a.size.width, b.size.width, 0.5)
        && within(a.size.height, b.size.height, 0.5)
}

/// `inner` lies within `outer`, to half a px.
fn inside(inner: &LogicalRect, outer: &LogicalRect) -> bool {
    inner.origin.x >= outer.origin.x - 0.5
        && inner.origin.y >= outer.origin.y - 0.5
        && inner.origin.x + inner.size.width <= outer.origin.x + outer.size.width + 0.5
        && inner.origin.y + inner.size.height <= outer.origin.y + outer.size.height + 0.5
}

// ---- the tests ----

/// The graph box is as big as the wrapper the editor's row gives the graph.
///
/// The row is 800 x 560 px below the 40 px bar; the side bar takes 50 px of it,
/// so the wrapper (`flex-grow: 1`, stretched across the row) is 750 x 560 px at
/// (50, 40). The graph box inside it declares `flex-grow: 1` and no size: it is
/// the wrapper's 750 x 560 px at (50, 40) when the wrapper is a flex container,
/// and 750 x 0 px when it is a block container (its height is its content's,
/// and its content is absolutely positioned).
///
/// Mutation: the wrapper without `display: flex` - the graph box is 750 x 0 px.
#[test]
fn the_graph_box_is_as_big_as_the_wrapper_the_editors_row_gives_the_graph() {
    for how in BUILDS {
        let lw = window_of(three_nodes(), how);
        let editor = rect_of(&lw, node_with_id(&lw, "editor"));
        assert!(
            same_rect(&editor, &rect(0.0, 40.0, 800.0, 560.0)),
            "harness ({how}): the editor row is the window below the 40 px bar; {}",
            boxes(&lw)
        );
        let graph_box = graph_box_of(&lw);
        let wrapper = rect_of(&lw, parent_of(&lw, graph_box));
        assert!(
            same_rect(&wrapper, &rect(50.0, 40.0, 750.0, 560.0)),
            "harness ({how}): the wrapper is the row beside the 50 px side bar; {}",
            boxes(&lw)
        );
        let graph_box = rect_of(&lw, graph_box);
        assert!(
            graph_box.size.height > 0.0,
            "({how}): the graph box has no height, so its overflow: hidden clips every node; {}",
            boxes(&lw)
        );
        assert!(
            same_rect(&graph_box, &wrapper),
            "({how}): the graph box is not the size of its wrapper; {}",
            boxes(&lw)
        );
    }
}

/// Every node box is laid out inside the graph box, the box that clips it.
///
/// A node box stands at the origin of the graph box before its `transform`
/// (the nodes are absolutely positioned and the boxes carry their places in
/// `translate()`), about 270 x 80 px: it is inside the graph box of 750 x 560
/// px, and not inside the graph box of 750 x 0 px that a block wrapper leaves.
///
/// Mutation: the wrapper without `display: flex` - the node boxes reach about
/// 80 px below a graph box that ends where they begin.
#[test]
fn every_node_box_is_laid_out_inside_the_graph_box_that_clips_it() {
    for how in BUILDS {
        let lw = window_of(three_nodes(), how);
        let graph_box = rect_of(&lw, graph_box_of(&lw));
        let nodes = nodes_with_class(&lw, "node_graph_node");
        assert_eq!(
            nodes.len(),
            AT.len(),
            "harness ({how}): a box for each node of the graph; {}",
            boxes(&lw)
        );
        for (i, node) in nodes.into_iter().enumerate() {
            let node_box = rect_of(&lw, node);
            assert!(
                node_box.size.width > 0.0 && node_box.size.height > 0.0,
                "harness ({how}): node {i} has a box; {}",
                boxes(&lw)
            );
            assert!(
                inside(&node_box, &graph_box),
                "({how}): the box of node {i}, {node_box:?}, is outside the graph box, {graph_box:?}, \
                 which clips it; {}",
                boxes(&lw)
            );
        }
    }
}

/// Every node is shown inside the graph box, where the graph puts it.
///
/// The box of a node is moved by `translate(x, y)` of its position in the graph
/// (the graph's offset, nothing here, rides on the container of the nodes): node
/// 2, at (320, 60) in the graph, is shown from (50 + 320, 40 + 60) = (370, 100),
/// about 270 px wide. That is inside the graph box (50, 40, 750 x 560), and
/// inside the window. The same place lies below a graph box of 750 x 0 px - it
/// is clipped.
///
/// Mutation: the wrapper without `display: flex` - node 2 is shown at (370, 100)
/// under a graph box that ends at y = 40.
#[test]
fn every_node_is_shown_where_the_graph_puts_it_inside_the_graph_box() {
    let window = rect(0.0, 0.0, WINDOW_W, WINDOW_H);
    for how in BUILDS {
        let lw = window_of(three_nodes(), how);
        let graph_box = rect_of(&lw, graph_box_of(&lw));
        let nodes = nodes_with_class(&lw, "node_graph_node");
        assert_eq!(
            nodes.len(),
            AT.len(),
            "harness ({how}): a box for each node of the graph; {}",
            boxes(&lw)
        );
        for (i, (node, at)) in nodes.into_iter().zip(AT.iter()).enumerate() {
            let shown = on_screen(&lw, node).unwrap_or_else(|| {
                panic!(
                    "harness ({how}): node {i} has no place on screen (no hit-test area); {}",
                    boxes(&lw)
                )
            });
            assert!(
                within(shown.origin.x, graph_box.origin.x + at.0, 1.0)
                    && within(shown.origin.y, graph_box.origin.y + at.1, 1.0),
                "harness ({how}): node {i} is shown at {:?}, not at its place ({}, {}) in the graph \
                 box; {}",
                shown.origin,
                at.0,
                at.1,
                boxes(&lw)
            );
            assert!(
                inside(&shown, &graph_box),
                "({how}): node {i} is shown at {shown:?}, outside the graph box, {graph_box:?}, \
                 which clips it; {}",
                boxes(&lw)
            );
            assert!(
                inside(&shown, &window),
                "({how}): node {i} is shown at {shown:?}, outside the window; {}",
                boxes(&lw)
            );
        }
    }
}

/// How many pixels of two frames of the window differ.
#[cfg(feature = "cpurender")]
fn differing(a: &crate::cpurender::AzulPixmap, b: &crate::cpurender::AzulPixmap) -> usize {
    assert_eq!(
        (a.width, a.height),
        (b.width, b.height),
        "harness: two frames of one window"
    );
    a.data()
        .chunks_exact(4)
        .zip(b.data().chunks_exact(4))
        .filter(|(one, other)| one != other)
        .count()
}

/// The frame the CPU renderer paints of the window.
#[cfg(feature = "cpurender")]
fn frame_of(lw: &LayoutWindow) -> crate::cpurender::AzulPixmap {
    let mut backend = crate::cpu_backend::CpuBackend::new();
    let _ = backend.render_frame(lw, &RendererResources::default(), WINDOW_W, WINDOW_H, 1.0);
    backend
        .last_frame
        .as_ref()
        .expect("harness: a frame")
        .clone_pixmap()
}

/// The nodes are painted: the frame of the graph with its three nodes is not
/// the frame of the graph without them. Three boxes of about 270 x 80 px are
/// some 65 000 px of the frame; a graph box of 0 px clips all of them, and the
/// frames are one.
///
/// Mutation: the wrapper without `display: flex` - the two frames are the same.
#[cfg(feature = "cpurender")]
#[test]
fn the_nodes_of_the_graph_are_painted_into_the_window() {
    for how in BUILDS {
        let with_nodes = frame_of(&window_of(three_nodes(), how));
        let without = frame_of(&window_of(graph_of(&[]), how));
        let differ = differing(&with_nodes, &without);
        assert!(
            differ > 1000,
            "({how}): the graph with its three nodes is painted as the graph without them: only \
             {differ} px of the frames differ"
        );
    }
}
