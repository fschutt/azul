//! A popover fits the room it has.
//!
//! A `<transient-window>` comes up at its content's size, but never bigger
//! than the room on the screen (or than its own `max-width` /
//! `max-height`): what does not fit then scrolls inside the window, so no
//! part of the popover lands off the screen and nothing in it is out of
//! reach. One that fits is untouched.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeData, NodeId, NodeType},
    geom::{LogicalSize, OptionLogicalSize},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
    transient::{fit_root_to, extract_subtree_as_dom, TransientAnchor, TransientWindowConfig},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    solver3::display_list::DisplayListItem,
    transient::transient_dom_id,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// The room the screen leaves the popover in these tests.
const ROOM: LogicalSize = LogicalSize {
    width: 400.0,
    height: 300.0,
};

/// `body > div > transient-window(popover_css) > rows x div(row_css)`.
fn page(popover_css: &str, rows: usize, row_css: &str) -> Dom {
    let config = TransientWindowConfig::opened().with_anchor(TransientAnchor::Bottom);
    let mut popover =
        Dom::create_from_data(NodeData::create_node(NodeType::TransientWindow(config)))
            .with_css(popover_css);
    for _ in 0..rows {
        popover = popover.with_child(Dom::create_div().with_css(row_css));
    }
    Dom::create_body()
        .with_css("margin: 0;")
        .with_child(Dom::create_div().with_css("width: 80px; height: 30px;").with_child(popover))
}

fn lay_out(mut dom: Dom, size: LogicalSize) -> LayoutWindow {
    let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = size;
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut Some(Vec::new()),
    )
    .unwrap();
    lw
}

fn popover_node(lw: &LayoutWindow) -> NodeId {
    let nodes = lw.layout_results[&DomId::ROOT_ID].styled_dom.node_data.as_container();
    (0..nodes.len())
        .map(NodeId::new)
        .find(|n| matches!(nodes[*n].get_node_type(), NodeType::TransientWindow(_)))
        .expect("the popover node")
}

/// The size the popover's window comes up at, in an 800 x 600 parent with
/// [`ROOM`] on the screen.
fn measured(popover_css: &str, rows: usize, row_css: &str) -> LogicalSize {
    let lw = lay_out(page(popover_css, rows, row_css), LogicalSize::new(800.0, 600.0));
    let node = popover_node(&lw);
    let ws = lw.current_window_state.clone();
    lw.layout_transient_content(node, transient_dom_id(0), OptionLogicalSize::None, &ws, ROOM)
        .expect("the popover measures")
}

/// The popover's content laid out in its own window of `size`, the way the
/// window does it: its root's box and its scroll frame (clip, content).
fn in_its_window(
    popover_css: &str,
    rows: usize,
    row_css: &str,
    size: LogicalSize,
) -> (LogicalSize, Option<(LogicalSize, LogicalSize)>) {
    let parent = lay_out(page(popover_css, rows, row_css), LogicalSize::new(800.0, 600.0));
    let node = popover_node(&parent);
    let mut content =
        extract_subtree_as_dom(&parent.layout_results[&DomId::ROOT_ID].styled_dom, node)
            .expect("extracts");
    fit_root_to(&mut content, size);
    let lw = lay_out(content, size);
    let root = lw
        .get_node_layout_rect(DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(0))),
        })
        .expect("the root was laid out")
        .size;
    let frame = lw.layout_results[&DomId::ROOT_ID]
        .display_list
        .items
        .iter()
        .find_map(|item| match item {
            DisplayListItem::PushScrollFrame {
                clip_bounds,
                content_size,
                ..
            } => Some((clip_bounds.inner().size, *content_size)),
            _ => None,
        });
    (root, frame)
}

const ROW: &str = "width: 120px; height: 20px;";

#[test]
fn a_popover_that_fits_comes_up_at_its_own_size_and_does_not_scroll() {
    let size = measured("", 2, ROW);
    assert_eq!(size, LogicalSize::new(120.0, 40.0), "two 120 x 20 rows");
    let (root, frame) = in_its_window("", 2, ROW, size);
    assert_eq!(root, size, "the root fills its window");
    assert!(
        frame.map_or(true, |(clip, content)| content.height <= clip.height
            && content.width <= clip.width),
        "nothing to scroll: {frame:?}"
    );
}

#[test]
fn a_popover_taller_than_its_room_comes_up_at_the_room_and_scrolls() {
    let size = measured("", 50, ROW);
    assert_eq!(size.height, ROOM.height, "50 rows of 20 px are capped at the room: {size:?}");
    assert!(size.width <= ROOM.width && size.width >= 120.0, "{size:?}");
    let (root, frame) = in_its_window("", 50, ROW, size);
    assert_eq!(root.height, ROOM.height, "the root is its window's height");
    let (clip, content) = frame.expect("the root scrolls");
    assert!(content.height >= 1000.0, "all 50 rows are in the scroll frame: {content:?}");
    assert!(
        content.width <= clip.width + 0.5,
        "the rows are not cut off at the side by the scroll bar: clip {clip:?}, content {content:?}"
    );
}

#[test]
fn a_popover_wider_than_its_room_is_as_wide_as_the_room() {
    let size = measured("", 1, "width: 900px; height: 20px;");
    assert_eq!(size.width, ROOM.width, "{size:?}");
    assert!(size.height <= ROOM.height, "{size:?}");
}

#[test]
fn a_popovers_own_max_height_caps_it_and_its_content_scrolls() {
    let size = measured("max-height: 120px;", 50, ROW);
    assert_eq!(size.height, 120.0, "{size:?}");
    let (root, frame) = in_its_window("max-height: 120px;", 50, ROW, size);
    assert_eq!(root.height, 120.0);
    let (_, content) = frame.expect("the root scrolls");
    assert!(content.height >= 1000.0, "{content:?}");
}

#[test]
fn a_popovers_own_max_height_beyond_the_room_gives_way_to_the_room() {
    let size = measured("max-height: 2000px;", 50, ROW);
    assert_eq!(size.height, ROOM.height, "{size:?}");
    let (root, _) = in_its_window("max-height: 2000px;", 50, ROW, size);
    assert_eq!(root.height, ROOM.height, "the root's own max gives way to its window");
}

#[test]
fn a_padded_popover_fits_the_room_with_its_padding() {
    let size = measured("padding: 10px;", 50, ROW);
    assert_eq!(size.height, ROOM.height, "the border box fits the room: {size:?}");
    let (root, _) = in_its_window("padding: 10px;", 50, ROW, size);
    assert_eq!(root.height, ROOM.height, "padding included, the root is its window's height");
}
