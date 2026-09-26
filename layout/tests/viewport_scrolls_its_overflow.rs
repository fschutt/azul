//! Content that overflows the WINDOW can be scrolled to.
//!
//! CSS Overflow 3 §3.3: the root element's `overflow: visible` is applied to
//! the VIEWPORT as `auto`. azul propagated that onto the root's computed
//! style, but the root box has `height: auto` - it grows to its content and
//! so never overflows ITSELF - and nothing else scrolled, so anything past
//! the window edge was unreachable. In the AzWidgets demo that is the page's
//! last 42px: an injected 26px menu bar above `<body>`, whose `height: 100%`
//! still resolves to the whole window, plus the UA's 8px body margins.

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
    task::Instant,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn laid_out(dom: Dom, w: f32, h: f32) -> LayoutWindow {
    let styled = StyledDom::create_from_dom(dom);
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(w, h);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled, &ws, &rr, &sc, &mut dbg)
        .unwrap();
    let now = Instant::from(std::time::Instant::now());
    azul_layout::managers::scroll_registration::register_scroll_nodes(&mut lw, &now);
    lw
}

/// The root's scrollable travel, or `None` when the root is not a scroll
/// container at all.
fn root_max_scroll_y(lw: &LayoutWindow) -> Option<f32> {
    lw.scroll_manager
        .get_scroll_state(DomId::ROOT_ID, NodeId::new(0))
        .map(|s| s.max_scroll_offsets().1)
}

/// A body that fills the window and carries the UA margins is 16px taller
/// than the window: a browser scrolls the viewport by exactly that.
#[test]
fn a_full_height_body_with_margins_scrolls_the_viewport() {
    let lw = laid_out(
        Dom::create_body().with_css("height: 100%; margin: 8px; background-color: #eee;"),
        400.0,
        700.0,
    );
    let max = root_max_scroll_y(&lw).expect("the viewport is a scroll container");
    assert!(
        (max - 16.0).abs() < 1.0,
        "the 8px margins above and below a full-height body are 16px of travel, got {max}"
    );
}

/// Chrome above the app (the injected software menu bar) pushes the body
/// down without shrinking it: the overflow grows by the chrome's height.
#[test]
fn chrome_above_a_full_height_body_adds_its_height_to_the_travel() {
    let dom = Dom::create_html()
        .with_child(Dom::create_div().with_css("height: 26px; background-color: #ccc;"))
        .with_child(Dom::create_body().with_css("height: 100%; margin: 8px;"));
    let lw = laid_out(dom, 800.0, 700.0);
    let max = root_max_scroll_y(&lw).expect("the viewport is a scroll container");
    assert!(
        (max - 42.0).abs() < 1.0,
        "26px of chrome + 16px of margins = 42px of travel, got {max}"
    );
}

/// A page that fits must NOT become a scroll container: no overlay bar, no
/// scroll node, nothing to scroll.
#[test]
fn a_page_that_fits_is_not_a_scroll_container() {
    let lw = laid_out(
        Dom::create_body().with_css("height: 100px; margin: 0;"),
        400.0,
        700.0,
    );
    assert_eq!(
        root_max_scroll_y(&lw).unwrap_or(0.0),
        0.0,
        "a page that fits has no travel"
    );
}

/// The viewport's horizontal travel, `0.0` when the root is no scroll
/// container at all.
fn root_max_scroll_x(lw: &LayoutWindow) -> f32 {
    lw.scroll_manager
        .get_scroll_state(DomId::ROOT_ID, NodeId::new(0))
        .map_or(0.0, |s| s.max_scroll_offsets().0)
}

/// `body > div.scroller > div(900px wide)` at 640x480, with the body laid
/// out as `body_display`. The scroller declares only `overflow-y: auto`,
/// which is how the AzWidgets page scroller is written.
fn a_wide_row_inside_a_y_scroller(body_display: &str) -> LayoutWindow {
    let dom = Dom::create_body()
        .with_css(&format!("height: 100%; margin: 0; {body_display}"))
        .with_child(
            Dom::create_div()
                .with_css("height: 300px; flex-grow: 1; min-height: 0; overflow-y: auto;")
                .with_child(Dom::create_div().with_css("width: 900px; height: 50px;")),
        );
    laid_out(dom, 640.0, 480.0)
}

/// Content inside a scroller is the SCROLLER's overflow, never the page's.
///
/// CSS Overflow 3 §3.1: when one axis is neither `visible` nor `clip`, a
/// `visible` on the other axis computes to `auto`. So `overflow-y: auto`
/// alone makes the box a scroll container on BOTH axes, and a row wider
/// than it scrolls inside it. Measured live in AzWidgets on macOS at
/// 640x480: a 733px Pagination inside the page scroller gave the scroller
/// 145px of horizontal travel - and the VIEWPORT another 177px, so a
/// sideways trackpad swipe dragged the whole body, custom titlebar and all.
#[test]
fn a_wide_row_inside_a_flex_items_y_scroller_does_not_widen_the_viewport() {
    let lw = a_wide_row_inside_a_y_scroller("display: flex; flex-direction: column;");
    assert_eq!(
        root_max_scroll_x(&lw),
        0.0,
        "the 900px row lives inside an `overflow-y: auto` scroller, so it must not give the \
         viewport horizontal travel"
    );
    let inner = lw
        .scroll_manager
        .get_scroll_state(DomId::ROOT_ID, NodeId::new(1))
        .expect("the scroller is registered")
        .max_scroll_offsets()
        .0;
    assert!(
        (inner - 260.0).abs() < 1.0,
        "the scroller itself scrolls the 900px row across its 640px, got {inner}"
    );
}

/// The same law for a scroller in normal block flow.
#[test]
fn a_wide_row_inside_a_block_y_scroller_does_not_widen_the_viewport() {
    let lw = a_wide_row_inside_a_y_scroller("display: block;");
    assert_eq!(
        root_max_scroll_x(&lw),
        0.0,
        "the 900px row lives inside an `overflow-y: auto` scroller, so it must not give the \
         viewport horizontal travel"
    );
}

/// An inner scroller keeps its own travel; the viewport's is separate.
#[test]
fn an_inner_scroller_and_the_viewport_scroll_independently() {
    let dom = Dom::create_body()
        .with_css("height: 100%; margin: 8px;")
        .with_child(
            Dom::create_div()
                .with_css("height: 100px; overflow-y: scroll;")
                .with_child(Dom::create_div().with_css("height: 400px;")),
        );
    let lw = laid_out(dom, 400.0, 700.0);
    let viewport = root_max_scroll_y(&lw).expect("the viewport scrolls its 16px of margin");
    assert!((viewport - 16.0).abs() < 1.0, "viewport travel {viewport}");
    let inner = lw
        .scroll_manager
        .get_scroll_state(DomId::ROOT_ID, NodeId::new(1))
        .expect("the inner scroller is registered")
        .max_scroll_offsets()
        .1;
    assert!(inner > 250.0, "the inner scroller keeps its own travel, got {inner}");
}
