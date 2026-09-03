//! A `VirtualView` sized by its content gets that size in the SAME frame.
//!
//! The solver sizes a `VirtualView` by what its callback reported on the
//! previous pass, and the callbacks run after the pass - so until now a
//! content-sized view was laid out at the 300x150 replaced-element default on
//! its first frame, and after an in-place re-render (`UpdateVirtualView`) it
//! kept the box its OLD content had earned: an auto-sized icon view painted a
//! 300x150 box until an unrelated relayout, and a status-bar label re-rendered
//! with a longer text was clipped.
//!
//! The contract under test: the box a content-sized view is laid out in is the
//! size its callback reported, on the first frame and after a re-render, at
//! the cost of exactly one extra pass per host dom; a steady-state pass costs
//! none; and a stated CSS size still wins over the report.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};

use azul_core::callbacks::{VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn};
use azul_core::dom::{Dom, DomId, DomNodeId, NodeType};
use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
use azul_core::id::NodeId;
use azul_core::refany::{OptionRefAny, RefAny};
use azul_core::resources::RendererResources;
use azul_core::styled_dom::{NodeHierarchyItemId, StyledDom};
use azul_core::FastBTreeSet;
use azul_layout::callbacks::ExternalSystemCallbacks;
use azul_layout::window::LayoutWindow;
use azul_layout::window_state::FullWindowState;
use rust_fontconfig::FcFontCache;

/// The "content" of the view under test: a box of the width the dataset
/// holds. Reported as the view's natural size, like an icon view reports its
/// glyph's measurement.
struct Label {
    width: f32,
}

static INVOCATIONS: AtomicU32 = AtomicU32::new(0);

extern "C" fn render_label(mut data: RefAny, _info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    INVOCATIONS.fetch_add(1, Ordering::SeqCst);
    let width = data.downcast_ref::<Label>().map_or(0.0, |l| l.width);
    let dom = Dom::create_div().with_css(format!("width: {width}px; height: 20px;").as_str());
    let rect = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(width, 20.0));
    VirtualViewReturn::with_dom(dom, rect, rect)
}

fn set_width(dataset: &RefAny, width: f32) {
    let mut dataset = dataset.clone();
    dataset.downcast_mut::<Label>().expect("the label dataset").width = width;
}

fn label_view(dataset: RefAny, css: &str) -> Dom {
    Dom::create_virtual_view(dataset.clone(), VirtualViewCallback::create(render_label))
        .with_dataset(OptionRefAny::Some(dataset))
        .with_css(css)
}

/// A status-bar-like flex row: text, the view, text.
fn window_with(view: Dom) -> (LayoutWindow, FullWindowState, NodeId) {
    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; align-items: center; height: 24px;")
            .with_child(Dom::create_div().with_css("width: 40px; height: 20px;"))
            .with_child(view)
            .with_child(Dom::create_div().with_css("width: 40px; height: 20px;")),
    );
    let (css, _) = azul_css::parser2::new_from_str("body { margin: 0; }");
    let styled_dom = StyledDom::create(&mut dom, css);
    let view_node = {
        let nodes = styled_dom.node_data.as_container();
        (0..nodes.len())
            .map(NodeId::new)
            .find(|n| matches!(nodes.get(*n).map(|d| d.get_node_type()), Some(NodeType::VirtualView)))
            .expect("the view is in the dom")
    };
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = window_state.clone();
    layout(&mut lw, styled_dom, &window_state);
    (lw, window_state, view_node)
}

fn layout(lw: &mut LayoutWindow, styled_dom: StyledDom, window_state: &FullWindowState) {
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());
    lw.layout_and_generate_display_list(
        styled_dom,
        window_state,
        &renderer_resources,
        &system_callbacks,
        &mut debug_messages,
    )
    .unwrap();
    if std::env::var("VV_TEST_DEBUG").is_ok() {
        for m in debug_messages.unwrap_or_default() {
            let t = m.message.as_str();
            if t.contains("irtual") || t.contains("second pass") || t.contains("css_dirty") || t.contains("intrinsic") {
                eprintln!("[dbg] {t}");
            }
        }
    }
}

fn node_rect(lw: &LayoutWindow, node: NodeId) -> LogicalRect {
    lw.get_node_layout_rect(DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(node)),
    })
    .expect("the node has a rect")
}

fn view_rect(lw: &LayoutWindow, node: NodeId) -> LogicalRect {
    node_rect(lw, node)
}

/// Re-render the view in place with the dataset's current content, the way
/// `CallbackInfo::trigger_virtual_view_rerender` does.
fn rerender(lw: &mut LayoutWindow, window_state: &FullWindowState, node: NodeId) {
    let mut set = FastBTreeSet::new();
    set.insert(node);
    let mut updates = BTreeMap::new();
    updates.insert(DomId::ROOT_ID, set);
    lw.queue_virtual_view_updates(updates);
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let updated =
        lw.process_pending_virtual_view_updates(window_state, &renderer_resources, &system_callbacks);
    assert_eq!(updated.len(), 1, "the queued view was re-invoked");
}

#[test]
fn content_sized_view_is_its_natural_size_on_the_first_frame() {
    let dataset = RefAny::new(Label { width: 64.0 });
    let (lw, _ws, node) = window_with(label_view(dataset, "display: inline-block;"));
    let rect = view_rect(&lw, node);
    assert!(
        (rect.size.width - 64.0).abs() < 0.5 && (rect.size.height - 20.0).abs() < 0.5,
        "the box is the reported natural size, not the 300x150 replaced default: {rect:?}"
    );
    assert!((rect.origin.y - 2.0).abs() < 0.5, "centered in the 24px row: {rect:?}");
    // Placed after the 40px sibling, and the sibling AFTER the view moved up
    // with it: the second pass re-flowed the host row, not just the view.
    assert!((rect.origin.x - 40.0).abs() < 0.5, "placed after the 40px sibling: {rect:?}");
    let after = node_rect(&lw, NodeId::new(node.index() + 1));
    assert!(
        (after.origin.x - 104.0).abs() < 0.5,
        "the trailing sibling follows the 64px view, not a 300px one: {after:?}"
    );
    assert_eq!(
        lw.frame_report.virtual_view_size_passes, 1,
        "exactly one extra pass folded the report in"
    );
}

#[test]
fn rerender_with_wider_content_grows_the_box_without_a_full_relayout() {
    let dataset = RefAny::new(Label { width: 64.0 });
    let (mut lw, ws, node) = window_with(label_view(dataset.clone(), "display: inline-block;"));
    assert!((view_rect(&lw, node).size.width - 64.0).abs() < 0.5);

    // "0 WORDS" -> "1234 WORDS": the dataset changes, the view is re-rendered
    // in place - no RefreshDom, no `layout_and_generate_display_list`.
    set_width(&dataset, 96.0);
    let passes_before = lw.frame_report.virtual_view_size_passes;
    let layouts_before = lw.frame_report.layout_passes;
    rerender(&mut lw, &ws, node);

    let rect = view_rect(&lw, node);
    assert!(
        (rect.size.width - 96.0).abs() < 0.5,
        "the box followed the wider content: {rect:?}"
    );
    let after = node_rect(&lw, NodeId::new(node.index() + 1));
    assert!(
        (after.origin.x - 136.0).abs() < 0.5,
        "the trailing sibling moved with the wider view: {after:?}"
    );
    assert_eq!(
        lw.frame_report.virtual_view_size_passes,
        passes_before + 1,
        "one in-place relayout of the host"
    );
    assert_eq!(
        lw.frame_report.layout_passes, layouts_before,
        "not a full `layout_and_generate_display_list` (that re-materializes every view)"
    );

    // Narrower again: shrinking is a size change too.
    set_width(&dataset, 30.0);
    rerender(&mut lw, &ws, node);
    let rect = view_rect(&lw, node);
    assert!((rect.size.width - 30.0).abs() < 0.5, "shrunk to the content: {rect:?}");
}

#[test]
fn rerender_with_unchanged_content_costs_no_pass() {
    let dataset = RefAny::new(Label { width: 64.0 });
    let (mut lw, ws, node) = window_with(label_view(dataset, "display: inline-block;"));
    let passes_before = lw.frame_report.virtual_view_size_passes;
    rerender(&mut lw, &ws, node);
    assert_eq!(
        lw.frame_report.virtual_view_size_passes, passes_before,
        "a report equal to the box is not stale"
    );
    assert!((view_rect(&lw, node).size.width - 64.0).abs() < 0.5);
}

#[test]
fn steady_state_pass_costs_no_extra_pass() {
    let dataset = RefAny::new(Label { width: 64.0 });
    let (mut lw, ws, node) = window_with(label_view(dataset, "display: inline-block;"));
    assert_eq!(lw.frame_report.virtual_view_size_passes, 1);
    // A full relayout (the shell's incremental_relayout / regenerate_layout)
    // on the SAME dom: the snapshot already carries 64x20, so the view is
    // laid out right the first time and the callback has nothing new to say.
    let styled_dom = lw.layout_results.remove(&DomId::ROOT_ID).unwrap().styled_dom;
    layout(&mut lw, styled_dom, &ws);
    assert_eq!(
        lw.frame_report.virtual_view_size_passes, 1,
        "no second pass once the box matches the report"
    );
    assert!((view_rect(&lw, node).size.width - 64.0).abs() < 0.5);
}

#[test]
fn a_stated_css_size_wins_over_the_report_and_costs_no_pass() {
    let dataset = RefAny::new(Label { width: 64.0 });
    let (lw, _ws, node) = window_with(label_view(
        dataset,
        "display: inline-block; width: 120px; height: 20px;",
    ));
    let rect = view_rect(&lw, node);
    assert!(
        (rect.size.width - 120.0).abs() < 0.5,
        "a stated width is the box, like on an <img>: {rect:?}"
    );
    assert_eq!(
        lw.frame_report.virtual_view_size_passes, 0,
        "a mismatch on a stated axis is not a stale box"
    );
}

#[test]
fn a_scrolling_view_is_never_content_sized() {
    // A view whose materialized window is smaller than its document (a page
    // list) is sized from the outside; its report is a window, not a claim.
    extern "C" fn render_pages(_data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
        let bounds = info.bounds.get_logical_size();
        let dom = Dom::create_div().with_css("width: 500px; height: 3000px;");
        let _ = bounds;
        VirtualViewReturn::with_dom(
            dom,
            LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(500.0, 3000.0)),
            LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(500.0, 30000.0)),
        )
    }
    let dataset = RefAny::new(());
    let view = Dom::create_virtual_view(dataset.clone(), VirtualViewCallback::create(render_pages))
        .with_dataset(OptionRefAny::Some(dataset))
        .with_css("display: block; width: 100%; height: 400px;");
    let (lw, _ws, node) = window_with(view);
    assert_eq!(lw.frame_report.virtual_view_size_passes, 0);
    let rect = view_rect(&lw, node);
    assert!((rect.size.height - 400.0).abs() < 0.5, "{rect:?}");
}

#[test]
fn a_box_constrained_by_css_converges_without_a_pass_per_rerender() {
    // `align-items: stretch` (the flex default): the row stretches the view
    // to 24px tall, so its box never equals its 20px report. That is not a
    // stale box - the solver had the report and CSS overruled it - and must
    // not cost a pass on every re-render.
    let dataset = RefAny::new(Label { width: 64.0 });
    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; height: 24px;")
            .with_child(label_view(dataset.clone(), "display: inline-block;")),
    );
    let (css, _) = azul_css::parser2::new_from_str("body { margin: 0; }");
    let styled_dom = StyledDom::create(&mut dom, css);
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = ws.clone();
    layout(&mut lw, styled_dom, &ws);
    let node = NodeId::new(2);
    let rect = view_rect(&lw, node);
    assert!((rect.size.width - 64.0).abs() < 0.5, "{rect:?}");
    assert!((rect.size.height - 24.0).abs() < 0.5, "stretched by the row: {rect:?}");
    assert_eq!(lw.frame_report.virtual_view_size_passes, 1, "the width needed one pass");

    rerender(&mut lw, &ws, node);
    rerender(&mut lw, &ws, node);
    assert_eq!(
        lw.frame_report.virtual_view_size_passes, 1,
        "the report the solver already had is not stale, however the row stretched the box"
    );
}
