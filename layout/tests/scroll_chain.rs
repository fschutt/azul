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

// ---------------------------------------------------------------------------
// A stacking context inside a scroll box that is not one
// ---------------------------------------------------------------------------

const SCROLL_BOX: NodeId = NodeId::new(1);
const TRANSLUCENT_BLOCK: NodeId = NodeId::new(3);

/// Red at half opacity over white.
fn is_translucent_red(px: (u8, u8, u8)) -> bool {
    px.0 > 200 && px.1 > 60 && px.1 < 190 && px.2 > 60 && px.2 < 190
}

fn is_white(px: (u8, u8, u8)) -> bool {
    px.0 > 240 && px.1 > 240 && px.2 > 240
}

/// A translucent block is a stacking context of its own, and the display
/// list paints stacking contexts with their parent context's children -
/// here the root's, after everything in flow. The scroll box around it is
/// not a stacking context, so its clip and scroll frame had long been closed
/// by then: the block was painted unscrolled and unclipped, while the hit
/// tester (and CSS) put it inside the box, scrolled and clipped with the
/// rest of its content.
///
/// `body > box(200x100, auto) > [50px, translucent red 300x50, 300px]`, the
/// box scrolled by 50: the block laid out at y=50 is under y=25, and what
/// sticks out past the box's right edge is clipped.
#[test]
fn a_translucent_block_in_a_scrolled_box_is_painted_scrolled_and_clipped_with_it() {
    let mut lw = window_with(
        Dom::create_body().with_css("margin: 0;").with_child(
            Dom::create_div()
                .with_css("width: 200px; height: 100px; overflow: auto;")
                .with_child(Dom::create_div().with_css("height: 50px;"))
                .with_child(Dom::create_div().with_css(
                    "width: 300px; height: 50px; background-color: #ff0000; opacity: 0.5;",
                ))
                .with_child(Dom::create_div().with_css("height: 300px;")),
        ),
    );
    scroll_to(&mut lw, SCROLL_BOX, LogicalPosition::new(0.0, 50.0));

    assert_eq!(
        node_under(&lw, LogicalPosition::new(100.0, 25.0)),
        Some(TRANSLUCENT_BLOCK),
        "scrolled by 50, the pointer at y=25 finds the block laid out at y=50"
    );
    let frame = render(&lw);
    let px = pixel(&frame, 100, 25);
    assert!(
        is_translucent_red(px),
        "the block the pointer finds at y=25 must be painted there, got the pixel {px:?}: it was \
         painted outside the scroll box's frame, where it was laid out"
    );
    let px = pixel(&frame, 250, 75);
    assert!(
        is_white(px),
        "the block is clipped by the box it sits in (x < 200), got the pixel {px:?} at (250, 75)"
    );
}

// ---------------------------------------------------------------------------
// `position: fixed` - the viewport is its containing block
// ---------------------------------------------------------------------------

const PAGE_ROOT: NodeId = NodeId::new(0);
const FIXED_BOX: NodeId = NodeId::new(1);

fn is_green(px: (u8, u8, u8)) -> bool {
    px.0 < 60 && px.1 > 200 && px.2 < 60
}

/// The viewport scrolled to `y` the way a wheel step lands: an immediate,
/// clamped `set_scroll_position` on the root element.
fn scroll_viewport_to(lw: &mut LayoutWindow, y: f32) {
    let travel = lw
        .scroll_manager
        .get_scroll_state(DomId::ROOT_ID, PAGE_ROOT)
        .expect("harness: the viewport scrolls a page taller than the window")
        .max_scroll_offsets()
        .1;
    assert!(travel >= y, "harness: the page has room for {y}px, got {travel}px");
    lw.scroll_manager.set_scroll_position(
        DomId::ROOT_ID,
        PAGE_ROOT,
        LogicalPosition::new(0.0, y),
        now(),
    );
    lw.scroll_manager.calculate_scrollbar_states();
}

/// A fixed box's containing block is the viewport (CSS Positioned Layout
/// §3.4): scrolling the page moves the page under it, never the box. It was
/// painted - and hit-tested - inside the page's scroll frame, so a fixed
/// header scrolled away with the text.
///
/// `body > [fixed green 100x50 at (0,0), blue 200px, red 100px, 700px]` in
/// a 400x300 window, the page scrolled by 150.
#[test]
fn a_fixed_box_stays_where_it_is_when_the_page_scrolls() {
    let mut lw = window_with(
        Dom::create_body()
            .with_css("margin: 0;")
            .with_child(Dom::create_div().with_css(
                "position: fixed; top: 0px; left: 0px; width: 100px; height: 50px; \
                 background-color: #00ff00;",
            ))
            .with_child(Dom::create_div().with_css("height: 200px; background-color: #0000ff;"))
            .with_child(Dom::create_div().with_css("height: 100px; background-color: #ff0000;"))
            .with_child(Dom::create_div().with_css("height: 700px;")),
    );
    scroll_viewport_to(&mut lw, 150.0);

    let frame = render(&lw);
    assert!(
        is_red(pixel(&frame, 200, 100)),
        "harness: the page itself scrolls - the red block laid out at y=200 is at y=50"
    );
    let px = pixel(&frame, 50, 25);
    assert!(
        is_green(px),
        "the fixed box must stay at the top of the window, got the pixel {px:?} at (50, 25): \
         it scrolled away with the page"
    );
    assert_eq!(
        node_under(&lw, LogicalPosition::new(50.0, 25.0)),
        Some(FIXED_BOX),
        "the pointer at (50, 25) must find the fixed box painted there"
    );
    let rect = lw
        .get_node_rect_in_viewport(dom_node(FIXED_BOX))
        .expect("the fixed box is laid out");
    assert_eq!(
        rect.origin.y, 0.0,
        "the fixed box is on screen at y=0, its viewport rect says y={}",
        rect.origin.y
    );
}

/// A fixed box is not clipped by an `overflow: hidden` ancestor that is not
/// its containing block - CSS 2.2 §11.1.1 exempts every descendant whose
/// containing block is the viewport. Painted in the page's frames it is
/// whole; the pointer must find all of it.
///
/// `body > wrapper(100x50, hidden) > fixed green 200x40 at (0,0)`: the part
/// right of x=100 overhangs the wrapper.
#[test]
fn a_fixed_box_is_not_clipped_by_a_box_that_is_not_its_containing_block() {
    let lw = window_with(
        Dom::create_body().with_css("margin: 0;").with_child(
            Dom::create_div()
                .with_css("width: 100px; height: 50px; overflow: hidden;")
                .with_child(Dom::create_div().with_css(
                    "position: fixed; top: 0px; left: 0px; width: 200px; height: 40px; \
                     background-color: #00ff00;",
                )),
        ),
    );
    let px = pixel(&render(&lw), 150, 20);
    assert!(
        is_green(px),
        "the fixed box overhanging the wrapper must be painted there, got {px:?} at (150, 20)"
    );
    assert_eq!(
        node_under(&lw, LogicalPosition::new(150.0, 20.0)),
        Some(NodeId::new(2)),
        "the pointer at (150, 20) must find the fixed box painted there"
    );
}
