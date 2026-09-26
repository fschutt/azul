//! The PAGE moves when the viewport scrolls.
//!
//! CSS Overflow 3 §3.3 gives the root element's overflow to the viewport, and
//! 443def41f made the viewport scroll what the root overflows: the scroll
//! manager registers the root with the window as its scrollport, and the
//! viewport's bar is painted and dragged. But the root never became a scroll
//! FRAME. `compute_scroll_ids` and `push_node_clips` read the root's own
//! `overflow: visible`, so the root got no scroll id and its content no
//! `PushScrollFrame`: a thumb drag moved the offset and the thumb while both
//! renderers painted the page where it was, the hit tester tested it there,
//! the wheel found no scroll container under the pointer, and neither a
//! reveal nor the caret could scroll a node below the window into view.
//!
//! The page: `body` without margins holding a 200px blue block, a 100px red
//! one, 600px of nothing and a 100px green one - 1000px in a 400x300 window,
//! 700px of travel. Every test scrolls by [`N`] (or asks the engine to) and
//! looks for the page [`N`] px higher.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId, TabIndex},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::{RendererResources, SystemAnimations},
    selection::{CursorAffinity, GraphemeClusterId, TextCursor},
    styled_dom::{NodeHierarchyItemId, StyledDom},
    task::Instant,
    transform::ComputedTransform3D,
};
use azul_css::props::basic::ColorU;
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    cpurender,
    glyph_cache::GlyphCache,
    headless::{convert_cpu_hit_test_to_full, CpuHitTester},
    managers::{
        hover::InputPointId,
        scroll_into_view::ScrollIntoViewOptions,
        scroll_state::{ScrollInputDevice, ScrollInputSource},
    },
    solver3::display_list::DisplayListItem,
    window::{LayoutWindow, ScrollMode, SelectionScrollType},
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const W: f32 = 400.0;
const H: f32 = 300.0;
/// How far the viewport is scrolled.
const N: f32 = 150.0;

/// The root element of the root DOM: the node whose overflow is the viewport's.
const ROOT: NodeId = NodeId::new(0);
const BLUE_NODE: NodeId = NodeId::new(1);
const RED_NODE: NodeId = NodeId::new(2);
const GREEN_NODE: NodeId = NodeId::new(4);

const BLUE: ColorU = ColorU {
    r: 0,
    g: 0,
    b: 255,
    a: 255,
};
const RED: ColorU = ColorU {
    r: 255,
    g: 0,
    b: 0,
    a: 255,
};

/// `body > [blue 200px, red 100px, 600px, green 100px]`: 1000px of page.
fn page() -> StyledDom {
    StyledDom::create_from_dom(
        Dom::create_body()
            .with_css("margin: 0;")
            .with_child(Dom::create_div().with_css("height: 200px; background-color: #0000ff;"))
            .with_child(Dom::create_div().with_css("height: 100px; background-color: #ff0000;"))
            .with_child(Dom::create_div().with_css("height: 600px;"))
            .with_child(Dom::create_div().with_css("height: 100px; background-color: #00ff00;")),
    )
}

/// A new 400x300 window with `styled` laid out once, the way the shells run
/// a pass: the window state first, then the funnel, which publishes the
/// scroll state.
fn window_with(styled: StyledDom, fonts: FcFontCache) -> LayoutWindow {
    let mut lw = LayoutWindow::new(fonts).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(W, H);
    lw.current_window_state = ws.clone();
    let mut debug = None;
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut debug,
    )
    .expect("the page lays out");
    lw
}

fn window() -> LayoutWindow {
    window_with(page(), FcFontCache::default())
}

fn now() -> Instant {
    Instant::from(std::time::Instant::now())
}

/// The viewport's current offset.
fn viewport_offset(lw: &LayoutWindow) -> LogicalPosition {
    lw.scroll_manager
        .get_current_offset(DomId::ROOT_ID, ROOT)
        .unwrap_or_default()
}

/// The viewport scrolled to `y` the way a wheel step lands: the physics
/// timer's `ScrollTo` is an immediate `scroll_to` (`set_scroll_position`) and
/// a refresh of the scrollbar geometry. No relayout, no new display list -
/// exactly like a real scroll, the renderers move the page from the offsets
/// alone.
fn scroll_viewport_to(lw: &mut LayoutWindow, y: f32) {
    let travel = lw
        .scroll_manager
        .get_scroll_state(DomId::ROOT_ID, ROOT)
        .expect("harness: the viewport scrolls a page taller than the window")
        .max_scroll_offsets()
        .1;
    assert!(
        travel >= y,
        "harness: 1000px of page in a 300px window has room for {y}px, got {travel}px"
    );
    lw.scroll_manager.set_scroll_position(
        DomId::ROOT_ID,
        ROOT,
        LogicalPosition::new(0.0, y),
        now(),
    );
    lw.scroll_manager.calculate_scrollbar_states();
}

/// The scroll offsets the renderers are handed, keyed by scroll id: the CPU
/// backends' `ScrollOffsetMap` and what `wr_translate2::scroll_all_nodes`
/// hands WebRender.
fn offsets(lw: &LayoutWindow) -> cpurender::ScrollOffsetMap {
    let lr = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the page is laid out");
    lw.scroll_manager
        .build_scroll_offset_map(DomId::ROOT_ID, &lr.scroll_id_to_node_id)
}

/// Where the root display list PAINTS the first rect of `color`, in window
/// space: its bounds minus the offsets of every scroll frame around it -
/// `pos - offset`, the rule the CPU raster paints with and the translation
/// WebRender applies to a scroll frame's content.
fn painted_rect(lw: &LayoutWindow, color: ColorU) -> LogicalRect {
    let lr = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the page is laid out");
    let offsets = offsets(lw);
    let mut frames: Vec<(f32, f32)> = Vec::new();
    for item in &lr.display_list.items {
        match item {
            DisplayListItem::PushScrollFrame { scroll_id, .. } => {
                frames.push(offsets.get(scroll_id).copied().unwrap_or((0.0, 0.0)));
            }
            DisplayListItem::PopScrollFrame => {
                frames.pop();
            }
            DisplayListItem::Rect {
                bounds, color: c, ..
            } if *c == color => {
                let (dx, dy) = frames
                    .iter()
                    .fold((0.0_f32, 0.0_f32), |(x, y), (fx, fy)| (x + fx, y + fy));
                let r = *bounds.inner();
                return LogicalRect::new(
                    LogicalPosition::new(r.origin.x - dx, r.origin.y - dy),
                    r.size,
                );
            }
            _ => {}
        }
    }
    panic!("harness: the page paints a {color:?} rect");
}

/// The window as the CPU backends' full-frame path draws it: the layered
/// compositor (`allocate_layers_from_display_list` -> `render_layers` ->
/// `composite_frame`), the same door `CallbackInfo::take_screenshot` goes
/// through, with the live scroll offsets.
fn render(lw: &LayoutWindow) -> cpurender::AzulPixmap {
    let lr = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the page is laid out");
    let state = cpurender::CpuRenderState::new(offsets(lw));
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

/// The layout-side hit tester every backend dispatches pointer input
/// through (`CommonWindowState::perform_hit_test`), rebuilt for this layout.
fn hit_tester(lw: &LayoutWindow) -> CpuHitTester {
    let mut tester = CpuHitTester::new();
    tester.rebuild_from_layout_with_gpu(&lw.layout_results, Some(&lw.gpu_state_manager));
    tester
}

/// The node the pointer finds at `at`: the topmost hit, resolved against the
/// live scroll offsets.
fn node_under(lw: &LayoutWindow, at: LogicalPosition) -> Option<NodeId> {
    let tester = hit_tester(lw);
    let resolve = |d: DomId, n: NodeId| lw.scroll_manager.get_current_offset(d, n);
    let no_transform = |_: DomId, _: NodeId| -> Option<ComputedTransform3D> { None };
    tester
        .hit_test_scrolled(at, &resolve, &no_transform)
        .first()
        .map(|hit| hit.1)
}

/// The paint and the hit test move the page with the viewport: scrolled by
/// N, the red block (laid out at y=200) is painted at y=200-N, the pixel
/// that showed blue shows red, and the pointer there finds the red block.
#[test]
fn a_scrolled_viewport_paints_and_hit_tests_the_page_moved_up_by_its_offset() {
    let mut lw = window();
    let at = LogicalPosition::new(200.0, 100.0);
    assert_eq!(
        node_under(&lw, at),
        Some(BLUE_NODE),
        "harness: unscrolled, the pointer at y=100 is over the blue block"
    );

    scroll_viewport_to(&mut lw, N);

    let red = painted_rect(&lw, RED);
    assert_eq!(
        red.origin.y,
        200.0 - N,
        "scrolled by {N}px, the red block laid out at y=200 must be painted at y={}, but the \
         display list paints it at y={} - no scroll frame moves the page",
        200.0 - N,
        red.origin.y
    );
    assert_eq!(
        painted_rect(&lw, BLUE).origin.y,
        -N,
        "the blue block moves with it"
    );

    let px = pixel(&render(&lw), 200, 100);
    assert!(
        px.0 > 200 && px.1 < 60 && px.2 < 60,
        "the CPU renderer must show the red block at y=100 once the page is scrolled by {N}px, \
         got the pixel {px:?}"
    );

    assert_eq!(
        node_under(&lw, at),
        Some(RED_NODE),
        "the pointer at y=100 must find the red block the page scrolled under it"
    );
}

/// A translucent block - its own compositor layer - moves with the page too.
///
/// The CPU compositor places every layer at its laid-out position within its
/// parent layer, and the page's frame covers the whole window. A layer
/// nested in it has to take the viewport's offset along wherever the frame
/// itself is painted.
#[test]
fn a_layer_inside_the_page_moves_with_the_viewport() {
    let styled = StyledDom::create_from_dom(
        Dom::create_body()
            .with_css("margin: 0;")
            .with_child(Dom::create_div().with_css("height: 200px;"))
            .with_child(
                Dom::create_div().with_css("height: 100px; background-color: #ff0000; opacity: 0.5;"),
            )
            .with_child(Dom::create_div().with_css("height: 700px;")),
    );
    let mut lw = window_with(styled, FcFontCache::default());
    scroll_viewport_to(&mut lw, N);
    let px = pixel(&render(&lw), 200, 100);
    assert!(
        px.0 > 200 && px.1 < 200 && px.2 < 200,
        "the half-transparent red block (laid out at y=200) must be composited at y=100 once \
         the page is scrolled by {N}px, got the pixel {px:?}"
    );
}

/// GUARD, holds today: the viewport's bar stays on top of the page.
///
/// The bar is painted after the page's content (`paint_scrollbars` runs
/// after `pop_node_clips`), and it must stay where it is while the content
/// scrolls. In the CPU compositor a scroll frame is its own layer,
/// composited OVER everything its parent layer paints - so a page frame
/// promoted to a layer would bury the bar under the page wherever the page
/// is opaque. The pixel under the bar's track, over the red block, is the
/// track's colour, not red.
#[test]
fn the_viewport_bar_stays_on_top_of_the_scrolled_page() {
    let mut lw = window();
    scroll_viewport_to(&mut lw, N);
    let px = pixel(&render(&lw), 395, 100);
    assert!(
        px.1 > 150 && px.2 > 150,
        "the viewport's bar (x 388..400) must be painted over the page, got the pixel {px:?} \
         at (395, 100)"
    );
}

/// GUARD, holds today: the page's frame clips nothing that a `visible`
/// root does not clip.
///
/// The root's box is 100px tall; its content overflows it by 900px, which
/// is what the viewport scrolls. The viewport's scrollport is the window -
/// not the root's box - so the page below the root's box is still painted.
#[test]
fn the_viewport_frame_clips_nothing_a_visible_root_leaves_visible() {
    let styled = StyledDom::create_from_dom(
        Dom::create_body()
            .with_css("margin: 0; height: 100px;")
            .with_child(Dom::create_div().with_css("height: 1000px; background-color: #0000ff;")),
    );
    let lw = window_with(styled, FcFontCache::default());
    let px = pixel(&render(&lw), 200, 200);
    assert!(
        px.0 < 60 && px.1 < 60 && px.2 > 200,
        "the blue block below the root's 100px box must be painted, got the pixel {px:?} at \
         (200, 200)"
    );
}

/// GUARD, holds today: a page that fits the window keeps the display list
/// it had - no viewport frame, no scroll id.
#[test]
fn a_page_that_fits_the_window_gets_no_viewport_frame() {
    let styled = StyledDom::create_from_dom(
        Dom::create_body()
            .with_css("margin: 0;")
            .with_child(Dom::create_div().with_css("height: 100px; background-color: #0000ff;")),
    );
    let lw = window_with(styled, FcFontCache::default());
    let lr = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the page is laid out");
    let frames = lr
        .display_list
        .items
        .iter()
        .filter(|item| matches!(item, DisplayListItem::PushScrollFrame { .. }))
        .count();
    assert_eq!(frames, 0, "a page that fits has nothing to scroll");
    assert!(
        lr.scroll_ids.is_empty(),
        "and no scroll id: {:?}",
        lr.scroll_ids
    );
}

/// A viewport scroll is a scroll SHIFT the CPU backends repaint.
///
/// Between two frames the display list does not change on a scroll; the CPU
/// backends find what moved by comparing the frames' offsets per scroll
/// frame (`collect_scroll_shifts`, shared by the shell and the e2e twin).
/// Without a frame for the page there was nothing to compare: a viewport
/// scroll repainted the thumb and left the page stale.
#[test]
fn a_viewport_scroll_is_a_scroll_shift_the_cpu_backends_repaint() {
    let mut lw = window();
    let before = offsets(&lw);
    scroll_viewport_to(&mut lw, N);
    let after = offsets(&lw);
    let lr = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the page is laid out");
    let shifts: Vec<(LogicalRect, (f32, f32))> =
        cpurender::collect_scroll_shifts(&lr.display_list, &after, &before, 1.0)
            .into_iter()
            .map(|(_, clip, delta, _)| (clip, delta))
            .collect();
    assert_eq!(
        shifts,
        vec![(
            LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(W, H)),
            (0.0, N)
        )],
        "scrolling the viewport by {N}px must move exactly one frame - the page, clipped to the \
         window"
    );
}

/// The wheel over the page scrolls the viewport.
///
/// `record_scroll_from_hit_test` is the platform wheel ingress: it picks the
/// innermost scroll container under the pointer from the hover hit test.
/// The CPU hit tester offered only nodes whose own overflow scrolls, so over
/// the page it offered nothing and the wheel was dropped.
#[test]
fn the_wheel_over_the_page_scrolls_the_viewport() {
    let mut lw = window();
    let at = LogicalPosition::new(200.0, 100.0);
    let tester = hit_tester(&lw);
    let hit = {
        let resolve = |d: DomId, n: NodeId| lw.scroll_manager.get_current_offset(d, n);
        let no_transform = |_: DomId, _: NodeId| -> Option<ComputedTransform3D> { None };
        let hits = tester.hit_test_scrolled(at, &resolve, &no_transform);
        convert_cpu_hit_test_to_full(
            &tester,
            &hits,
            None,
            &lw.layout_results,
            at,
            &resolve,
            &no_transform,
        )
    };
    lw.hover_manager.push_hit_test(InputPointId::Mouse, hit);
    // A wheel notch towards the user, as the platform reports it
    // (`record_scroll_input` applies the scroll direction).
    let target = lw
        .scroll_manager
        .record_scroll_from_hit_test(
            0.0,
            -40.0,
            ScrollInputSource::WheelDiscrete,
            ScrollInputDevice::MouseWheel,
            &lw.hover_manager,
            &InputPointId::Mouse,
            now(),
        )
        .map(|(dom, node, _)| (dom, node));
    assert_eq!(
        target,
        Some((DomId::ROOT_ID, ROOT)),
        "the wheel over the page must scroll the viewport"
    );
}

/// Scrolling a node below the window into view scrolls the viewport.
///
/// `scroll_into_view` walks the node's ancestors for scroll containers, and
/// asked each for its own overflow - the root's `visible`.
#[test]
fn scrolling_a_node_below_the_window_into_view_scrolls_the_viewport() {
    let mut lw = window();
    let green = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(GREEN_NODE)),
    };
    let adjustments =
        lw.scroll_node_into_view(green, ScrollIntoViewOptions::nearest().with_instant(), now());
    assert_eq!(
        viewport_offset(&lw).y,
        700.0,
        "the green block (y 900..1000) must be scrolled into the 300px window - the viewport \
         has to travel to its bottom; adjustments: {adjustments:?}"
    );
}

/// A caret below the window scrolls the viewport to reveal it.
///
/// The caret reveal measured the viewport's scroll container - the root -
/// against the root's own box, which is as tall as the page: the caret was
/// always "visible", and typing at the end of a long page never followed it.
#[test]
fn a_caret_below_the_window_scrolls_the_viewport_to_reveal_it() {
    let mut editor = Dom::create_div().with_css("display: block;");
    editor.set_contenteditable(true);
    editor.set_tab_index(TabIndex::Auto);
    // body(0) > spacer(1) + editor(2) > text(3)
    let styled = StyledDom::create_from_dom(
        Dom::create_body()
            .with_css("margin: 0; font-size: 16px;")
            .with_child(Dom::create_div().with_css("height: 900px;"))
            .with_child(editor.with_child(
                Dom::create_text_do_not_use_without_block_level_wrapper("the last line"),
            )),
    );
    let mut lw = window_with(styled, FcFontCache::build());
    // Instant reveal: a gliding one only queues the target for the physics.
    lw.system_animations_override = Some(SystemAnimations::disabled());
    lw.focus_manager.set_focused_node(Some(DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(2))),
    }));
    lw.text_edit_manager.initialize_editing(
        TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: 0,
            },
            affinity: CursorAffinity::Leading,
        },
        DomId::ROOT_ID,
        NodeId::new(3),
        0,
    );
    let caret = lw
        .get_focused_cursor_rect()
        .expect("harness: the caret has a rect");
    assert!(
        caret.origin.y >= 899.0,
        "harness: the caret sits below the 300px window, at {caret:?}"
    );

    let scrolled = lw.scroll_selection_into_view(SelectionScrollType::Cursor, ScrollMode::Instant);
    let offset = viewport_offset(&lw).y;
    assert!(
        scrolled,
        "the caret at y={} must be revealed, but the reveal found it visible (viewport offset \
         {offset})",
        caret.origin.y
    );
    let top = caret.origin.y - offset;
    let bottom = top + caret.size.height;
    assert!(
        top >= -0.5 && bottom <= H + 0.5,
        "after the reveal the caret must lie inside the window, it is at y {top}..{bottom} \
         (viewport offset {offset})"
    );
}
