//! A box is painted where the pointer finds it, whatever scrolls around it.
//!
//! Six places decided which ancestors' scroll offsets move a box: the
//! display list (it opened a scroll frame for `scroll | auto` only), the CPU
//! hit tester and `node_rect_to_screen` (every ancestor with a scroll id,
//! `overflow: hidden` included), the scroll manager's bar tracks,
//! `LayoutWindow::accumulated_scroll` (every ancestor with ANY scroll state)
//! and the DOM walk `find_scroll_parent`. They agreed while every offset sat
//! on a `scroll | auto` box whose descendants were all laid out in it. Each
//! test here scrolls something the rules disagreed about and checks the
//! painted pixel against the node the hit tester finds at the same point.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::{LogicalPosition, LogicalSize},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
    task::Instant,
    transform::ComputedTransform3D,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, cpurender, glyph_cache::GlyphCache,
    headless::CpuHitTester, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const W: f32 = 400.0;
const H: f32 = 300.0;

/// A new 400x300 window with `dom` laid out once, the way the shells run a
/// pass: the window state first, then the funnel, which publishes the
/// scroll state.
fn window_with(dom: Dom) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(W, H);
    lw.current_window_state = ws.clone();
    let mut debug = None;
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut debug,
    )
    .expect("the page lays out");
    lw
}

fn now() -> Instant {
    Instant::from(std::time::Instant::now())
}

fn dom_node(node: NodeId) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(node)),
    }
}

/// `node` of the root dom scrolled to `offset` by a program - what
/// `CallbackInfo::scroll_to_unclamped` stores. No relayout, no new display
/// list: like any scroll, the renderers and the hit tester move content
/// from the offsets alone.
fn scroll_to(lw: &mut LayoutWindow, node: NodeId, offset: LogicalPosition) {
    lw.scroll_manager
        .set_scroll_position_unclamped(DomId::ROOT_ID, node, offset, now());
    lw.scroll_manager.calculate_scrollbar_states();
}

/// The window as the CPU backends' full-frame path draws it: the layered
/// compositor, with the live scroll offsets - the door
/// `CallbackInfo::take_screenshot` goes through.
fn render(lw: &LayoutWindow) -> cpurender::AzulPixmap {
    let lr = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the page is laid out");
    let offsets = lw
        .scroll_manager
        .build_scroll_offset_map(DomId::ROOT_ID, &lr.scroll_id_to_node_id);
    let state = cpurender::CpuRenderState::new(offsets);
    let (w, h) = (W as u32, H as u32);
    let mut compositor = cpurender::CompositorState::new(w, h);
    compositor.allocate_layers_from_display_list(
        &lr.display_list,
        1.0,
        &state.transforms,
        &state.opacities,
    );
    compositor
        .render_layers(
            &lr.display_list,
            1.0,
            &RendererResources::default(),
            &lw.font_manager,
            &mut GlyphCache::new(),
            &state,
        )
        .expect("the page renders");
    let mut out = cpurender::AzulPixmap::new(w, h).expect("a pixmap");
    out.fill(255, 255, 255, 255);
    compositor.composite_frame(&mut out, 1.0);
    out
}

/// The RGB of one pixel.
fn pixel(p: &cpurender::AzulPixmap, x: u32, y: u32) -> (u8, u8, u8) {
    let i = ((y * p.width() + x) * 4) as usize;
    let d = p.data();
    (d[i], d[i + 1], d[i + 2])
}

fn is_red(px: (u8, u8, u8)) -> bool {
    px.0 > 200 && px.1 < 60 && px.2 < 60
}

fn is_blue(px: (u8, u8, u8)) -> bool {
    px.0 < 60 && px.1 < 60 && px.2 > 200
}

/// The node the pointer finds at `at`: the topmost hit of the layout-side
/// hit tester every backend dispatches pointer input through, against the
/// live scroll offsets.
fn node_under(lw: &LayoutWindow, at: LogicalPosition) -> Option<NodeId> {
    let mut tester = CpuHitTester::new();
    tester.rebuild_from_layout_with_gpu(&lw.layout_results, Some(&lw.gpu_state_manager));
    let resolve = |d: DomId, n: NodeId| lw.scroll_manager.get_current_offset(d, n);
    let no_transform = |_: DomId, _: NodeId| -> Option<ComputedTransform3D> { None };
    tester
        .hit_test_scrolled(at, &resolve, &no_transform)
        .first()
        .map(|hit| hit.1)
}

// ---------------------------------------------------------------------------
// `overflow: hidden` - a scroll container a program can scroll
// ---------------------------------------------------------------------------

const HIDDEN_BOX: NodeId = NodeId::new(1);
const RED_BLOCK: NodeId = NodeId::new(2);
const BLUE_BLOCK: NodeId = NodeId::new(3);

/// CSS Overflow 3 §3.1: an `overflow: hidden` box is a scroll container. The
/// user cannot scroll it, a program can - and then its content has to be
/// painted where the pointer finds it.
///
/// `body > box(200x100, hidden) > [red 50px, blue 250px]`, the box scrolled
/// by 50: the blue block (laid out at y=50) is under y=25.
#[test]
fn an_overflowing_hidden_box_scrolled_by_a_program_is_painted_where_the_pointer_finds_it() {
    let mut lw = window_with(
        Dom::create_body().with_css("margin: 0;").with_child(
            Dom::create_div()
                .with_css("width: 200px; height: 100px; overflow: hidden;")
                .with_child(Dom::create_div().with_css("height: 50px; background-color: #ff0000;"))
                .with_child(
                    Dom::create_div().with_css("height: 250px; background-color: #0000ff;"),
                ),
        ),
    );
    let at = LogicalPosition::new(100.0, 25.0);
    assert!(
        is_red(pixel(&render(&lw), 100, 25)),
        "harness: unscrolled, the red block is painted at y=25"
    );
    assert_eq!(
        node_under(&lw, at),
        Some(RED_BLOCK),
        "harness: unscrolled, the pointer at y=25 is over the red block"
    );

    scroll_to(&mut lw, HIDDEN_BOX, LogicalPosition::new(0.0, 50.0));

    assert_eq!(
        node_under(&lw, at),
        Some(BLUE_BLOCK),
        "scrolled by 50, the pointer at y=25 finds the blue block laid out at y=50"
    );
    let px = pixel(&render(&lw), 100, 25);
    assert!(
        is_blue(px),
        "the blue block the pointer finds at y=25 must be painted there too, got the pixel \
         {px:?}: the hit tester moves the hidden box's content by its offset, the display list \
         opened no scroll frame for it and painted it unscrolled"
    );
}

/// An `overflow: hidden` box whose content fits cannot scroll - its range
/// is zero, and it is painted without a scroll frame. An offset a program
/// stores on it anyway (`scroll_to_unclamped`) must then move nothing, for
/// the pointer and for the box's viewport rect alike, because nothing moved
/// on screen.
///
/// `body > [box(200x100, hidden) > red 100px, blue 100px]`, the box
/// "scrolled" by 50: y=75 is still painted red.
#[test]
fn a_hidden_box_whose_content_fits_is_not_moved_by_an_offset() {
    let mut lw = window_with(
        Dom::create_body()
            .with_css("margin: 0;")
            .with_child(
                Dom::create_div()
                    .with_css("width: 200px; height: 100px; overflow: hidden;")
                    .with_child(
                        Dom::create_div().with_css("height: 100px; background-color: #ff0000;"),
                    ),
            )
            .with_child(Dom::create_div().with_css("height: 100px; background-color: #0000ff;")),
    );
    scroll_to(&mut lw, HIDDEN_BOX, LogicalPosition::new(0.0, 50.0));

    let px = pixel(&render(&lw), 100, 75);
    assert!(
        is_red(px),
        "harness: a box that cannot scroll is painted where it was laid out, got {px:?}"
    );
    assert_eq!(
        node_under(&lw, LogicalPosition::new(100.0, 75.0)),
        Some(RED_BLOCK),
        "the red block painted at y=75 must be what the pointer finds there - the hit tester \
         added the box's offset and landed below the block, on the box itself"
    );
    let rect = lw
        .get_node_rect_in_viewport(dom_node(RED_BLOCK))
        .expect("the red block is laid out");
    assert_eq!(
        rect.origin.y, 0.0,
        "the red block is on screen at y=0, but its viewport rect says y={}",
        rect.origin.y
    );
}
