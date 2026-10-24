//! A scroll box scrolled to the end shows its block-end padding.
//!
//! CSS Overflow 3 §2.2: the scrollable overflow of a scroll container is
//! measured in its PADDING box - its content, the margin boxes of its in-flow
//! children, and its end padding after them. The scroll range is that extent
//! minus the scrollport (the padding box itself).
//!
//! The range was the CONTENT's extent minus the padding box: the padding was
//! counted on the scrollport side and never on the content side, so a
//! scroll box could not be scrolled far enough to show its own bottom
//! padding - it fell short by `padding-top + padding-bottom`, plus the last
//! child's bottom margin where the extent left that out (a flex item's). The
//! widgets demo's page column (`overflow-y: auto; padding: 24px`, cards with
//! `margin-bottom: 20px`) stopped with its last card, "Date & Time", running
//! into the window's bottom edge, its lower 24 px cut off.

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// The scroll box: `body(0) > box(1) > card(2) .. card(1 + CARDS)`.
const BOX: NodeId = NodeId::new(1);
const CARDS: usize = 5;
/// The box's padding on every side.
const PADDING: f32 = 24.0;
/// Every card's bottom margin.
const MARGIN: f32 = 20.0;

/// Lays out `body > box > CARDS cards` in a 400 x 300 window.
fn lay_out(body_css: &str, box_css: &str, card_css: &str) -> LayoutWindow {
    let mut page = Dom::create_div().with_css(box_css);
    for _ in 0..CARDS {
        page = page.with_child(Dom::create_div().with_css(card_css));
    }
    let styled = StyledDom::create_from_dom(Dom::create_body().with_css(body_css).with_child(page));
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
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

/// How far above the box's bottom edge its last card's border box ends,
/// with the box scrolled all the way down (negative: the card is cut off).
fn gap_below_the_last_card(lw: &LayoutWindow) -> f32 {
    let lr = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the page is laid out");
    let top_and_height = |node: NodeId| -> (f32, f32) {
        let idx = *lr
            .layout_tree
            .dom_to_layout
            .get(&node)
            .and_then(|boxes| boxes.first())
            .expect("harness: the node has a box");
        let top = lr
            .calculated_positions
            .get(idx.index())
            .map(|p| p.y)
            .expect("harness: the box is positioned");
        let height = lr.layout_tree.nodes[idx.index()]
            .used_size
            .expect("harness: the box is sized")
            .height;
        (top, height)
    };
    let (box_top, box_height) = top_and_height(BOX);
    let (card_top, card_height) = top_and_height(NodeId::new(1 + CARDS));
    let max_scroll = lw
        .scroll_manager
        .get_scroll_state(DomId::ROOT_ID, BOX)
        .expect("harness: the box overflows, so it is a registered scroll box")
        .max_scroll_offsets()
        .1;
    (box_top + box_height) - (card_top + card_height - max_scroll)
}

/// The widgets demo's shape: a flex column page (`height: 100%`) whose body
/// column scrolls, with padding, over cards that keep their height.
#[test]
fn a_scrolled_flex_page_ends_with_its_last_margin_and_its_bottom_padding() {
    let lw = lay_out(
        "margin: 0; display: flex; flex-direction: column; height: 100%;",
        "display: flex; flex-direction: column; overflow-y: auto; flex-grow: 1; min-height: 0; \
         padding: 24px;",
        "height: 200px; margin-bottom: 20px; flex-shrink: 0;",
    );
    let gap = gap_below_the_last_card(&lw);
    assert!(
        (gap - (MARGIN + PADDING)).abs() < 0.5,
        "scrolled to the end, the last card must end {} px (its margin plus the box's bottom \
         padding) above the box's bottom edge, ends {gap} px above it",
        MARGIN + PADDING,
    );
}

/// The same in block flow: a fixed-height block scroll box with padding.
#[test]
fn a_scrolled_block_box_ends_with_its_last_margin_and_its_bottom_padding() {
    let lw = lay_out(
        "margin: 0;",
        "height: 252px; overflow-y: auto; padding: 24px;",
        "height: 200px; margin-bottom: 20px;",
    );
    let gap = gap_below_the_last_card(&lw);
    assert!(
        (gap - (MARGIN + PADDING)).abs() < 0.5,
        "scrolled to the end, the last card must end {} px (its margin plus the box's bottom \
         padding) above the box's bottom edge, ends {gap} px above it",
        MARGIN + PADDING,
    );
}
