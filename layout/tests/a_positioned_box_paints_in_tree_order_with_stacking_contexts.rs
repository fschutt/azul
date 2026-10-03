//! A positioned box paints in TREE ORDER with the stacking contexts of its
//! stacking context - after every in-flow box, never before an earlier
//! transformed sibling.
//!
//! CSS 2.2 Appendix E, step 8 of a stacking context's painting order: "All
//! positioned descendants with 'z-index: auto' or 'z-index: 0', in tree
//! order." A transformed element creates a stacking context and paints at
//! that same step (CSS Transforms 1, section 3: "as if it were a positioned
//! element with z-index: 0"), as does one with `opacity < 1`. The display
//! list painted a `position: absolute` / `relative` box with `z-index: auto`
//! among its parent's IN-FLOW children (step 4), so it went under every
//! later in-flow box and under every transformed or translucent sibling,
//! even an earlier one (`e2e/bug-transform-offsets-hit-test`: `#below` was
//! painted BEFORE the earlier `#mover`'s stacking context; HEADLESS6
//! 2026-10-03).
//!
//! The pixel test renders through the layered CPU compositor, the path the
//! CPU backends and the debug server's screenshot take: a transformed box is
//! a layer there, and a box the list paints after the layer must still come
//! out on top of it.

use std::collections::HashMap;

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    cpurender,
    glyph_cache::GlyphCache,
    solver3::display_list::{DisplayList, DisplayListItem},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const W: u32 = 200;
const H: u32 = 100;

fn laid_out(page: Dom) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(W as f32, H as f32);
    lw.current_window_state = ws.clone();
    let mut debug = None;
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(page),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut debug,
    )
    .expect("the page lays out");
    lw
}

fn display_list(lw: &LayoutWindow) -> &DisplayList {
    &lw.layout_results
        .get(&DomId::ROOT_ID)
        .expect("the root DOM is laid out")
        .display_list
}

/// Index of the first background rect the list paints for `node`.
fn first_rect_of(dl: &DisplayList, node: NodeId) -> usize {
    dl.items
        .iter()
        .enumerate()
        .position(|(i, item)| {
            matches!(item, DisplayListItem::Rect { .. })
                && dl.node_mapping.get(i).copied().flatten() == Some(node)
        })
        .unwrap_or_else(|| panic!("harness: node {node:?} paints a background rect"))
}

/// The RGBA pixel at `(x, y)` of the page rendered through the layered
/// compositor.
fn pixel_at(lw: &LayoutWindow, x: u32, y: u32) -> [u8; 4] {
    let dl = display_list(lw);
    let rr = RendererResources::default();
    let mut glyph_cache = GlyphCache::new();
    let render_state = cpurender::CpuRenderState::new(Default::default());
    let mut compositor = cpurender::CompositorState::new(W, H);
    compositor.allocate_layers_from_display_list(dl, 1.0, &HashMap::new(), &HashMap::new());
    compositor
        .render_layers(
            dl,
            1.0,
            &rr,
            &lw.font_manager,
            &mut glyph_cache,
            &render_state,
        )
        .expect("the layers render");
    let mut out = cpurender::AzulPixmap::new(W, H).expect("a pixmap");
    out.fill(255, 255, 255, 255);
    compositor.composite_frame(&mut out, 1.0);
    let i = ((y * W + x) * 4) as usize;
    let d = out.data();
    [d[i], d[i + 1], d[i + 2], d[i + 3]]
}

/// body(0) > mover(1) > box(2); body > below(3). `#mover` is translated by
/// (20, 10) and holds the red `#box`; `#below`, LATER in the tree, is an
/// absolutely positioned blue box at (0, 0). They overlap in (20..100,
/// 10..50).
const BOX: NodeId = NodeId::new(2);
const BELOW: NodeId = NodeId::new(3);

fn transformed_then_positioned() -> LayoutWindow {
    laid_out(
        Dom::create_body()
            .with_css("margin: 0; padding: 0;")
            .with_child(
                Dom::create_div()
                    .with_css("width: 100px; height: 50px; transform: translate(20px, 10px);")
                    .with_child(
                        Dom::create_div()
                            .with_css("width: 100px; height: 50px; background: #ff0000;"),
                    ),
            )
            .with_child(Dom::create_div().with_css(
                "position: absolute; left: 0px; top: 0px; width: 100px; height: 50px; \
                 background: #0000ff;",
            )),
    )
}

#[test]
fn a_positioned_box_is_painted_after_an_earlier_transformed_sibling() {
    let lw = transformed_then_positioned();
    let dl = display_list(&lw);
    let moved = first_rect_of(dl, BOX);
    let below = first_rect_of(dl, BELOW);
    assert!(
        moved < below,
        "the transformed #mover comes first in the tree and both paint at step 8 of the body's \
         stacking context, in tree order: its #box (item {moved}) must be painted before the \
         positioned #below (item {below})"
    );
}

#[test]
fn a_positioned_box_shows_over_an_earlier_transformed_sibling_in_the_cpu_compositor() {
    let lw = transformed_then_positioned();
    let [r, g, b, _] = pixel_at(&lw, 60, 30);
    assert!(
        b > 200 && r < 60 && g < 60,
        "at (60, 30) the moved red #box and the later blue #below overlap: blue is on top, got \
         rgb({r}, {g}, {b})"
    );
}

/// body(0) > overlay(1); body > after(2). `#overlay` is absolutely
/// positioned at (0, 0); `#after` is an in-flow block at the same place.
const OVERLAY: NodeId = NodeId::new(1);
const AFTER: NodeId = NodeId::new(2);

fn positioned_then_in_flow() -> LayoutWindow {
    laid_out(
        Dom::create_body()
            .with_css("margin: 0; padding: 0;")
            .with_child(Dom::create_div().with_css(
                "position: absolute; left: 0px; top: 0px; width: 100px; height: 50px; \
                 background: #0000ff;",
            ))
            .with_child(
                Dom::create_div().with_css("width: 100px; height: 50px; background: #ff0000;"),
            ),
    )
}

#[test]
fn a_positioned_box_is_painted_after_a_later_in_flow_box() {
    let lw = positioned_then_in_flow();
    let dl = display_list(&lw);
    let overlay = first_rect_of(dl, OVERLAY);
    let after = first_rect_of(dl, AFTER);
    assert!(
        after < overlay,
        "in-flow blocks paint at step 4, positioned boxes at step 8: the in-flow #after (item \
         {after}) must be painted before the absolutely positioned #overlay (item {overlay})"
    );
}

#[test]
fn a_positioned_box_shows_over_a_later_in_flow_box() {
    let lw = positioned_then_in_flow();
    let [r, g, b, _] = pixel_at(&lw, 50, 25);
    assert!(
        b > 200 && r < 60 && g < 60,
        "the absolutely positioned blue #overlay covers the in-flow red #after, got \
         rgb({r}, {g}, {b})"
    );
}
