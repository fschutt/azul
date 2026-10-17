//! The caret rectangle the input method is handed is where the raster
//! paints the caret.
//!
//! The raster paints an item at `T(static - scroll)`: the scroll offsets of
//! the frames it is painted in, then the FORWARD transforms of the
//! reference frames around it (`headless::node_rect_to_screen` resolves the
//! same chain for the a11y tree, menus and the hit tester).
//! `LayoutWindow::cursor_rect_viewport_for` - the IME's candidate-window
//! anchor, and `TextTarget::rect_to_window` behind every IME rect - applied
//! the scroll and then the INVERSE of a transform per layout ancestor, read
//! from `GpuValueCache::current_transform_values`. That map holds the
//! VERTICAL SCROLLBAR THUMB translations, keyed by scroll box, not CSS
//! transforms. So a caret under `transform: translateX(..)` was reported
//! where it is not painted, and a caret on a scrolled page was moved by the
//! page's thumb offset.
//!
//! The window is 400x300; every box has no margin or padding.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
    task::Instant,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const CSS: &str = r#"
    * { margin: 0; padding: 0; }
    body { font-size: 14px; width: 400px; }
    .wrap { display: block; transform: translateX(60px); }
    .host { display: block; }
    .spacer { display: block; height: 100px; }
    .tail { display: block; height: 900px; }
"#;

/// The contenteditable host is node 2 in both pages.
const HOST: usize = 2;

fn class(name: &str) -> azul_core::dom::IdOrClassVec {
    vec![IdOrClass::Class(name.into())].into()
}

fn host() -> Dom {
    Dom::create_div()
        .with_ids_and_classes(class("host"))
        .with_contenteditable(true)
        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
            "hello world",
        ))
}

fn layout(mut dom: Dom) -> LayoutWindow {
    let (css, _) = azul_css::parser2::new_from_str(CSS);
    let styled_dom = StyledDom::create(&mut dom, css);
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    let mut debug = Some(Vec::new());
    lw.layout_and_generate_display_list(
        styled_dom,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut debug,
    )
    .expect("the page lays out");
    lw
}

fn dnid(n: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    }
}

/// A click into the host's text opens the editing session, as a user's
/// does (the page is unscrolled, so the click's window point is the static
/// one), and the host takes the focus.
fn open_session(lw: &mut LayoutWindow) {
    let host = lw
        .get_node_layout_rect(dnid(HOST))
        .expect("the host is laid out");
    lw.process_mouse_click_for_selection(
        LogicalPosition::new(host.origin.x + 10.0, host.origin.y + host.size.height * 0.5),
        0,
    )
    .expect("the click lands in the host's text");
    lw.focus_manager.set_focused_node(Some(dnid(HOST)));
}

/// The caret's STATIC rect (layout space) and the rect the IME is handed.
fn static_and_reported(lw: &LayoutWindow) -> (LogicalRect, LogicalRect) {
    let at = lw
        .get_focused_cursor_rect()
        .expect("harness: the session has a caret");
    let reported = lw
        .get_focused_cursor_rect_viewport()
        .expect("harness: the caret has an on-screen rect");
    (at, reported)
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.5
}

/// body(0) > wrap(1) `transform: translateX(60px)` > host(2) > text. The
/// raster paints the caret 60px right of its layout position.
#[test]
fn a_caret_under_a_translated_box_is_reported_where_it_is_painted() {
    let mut lw = layout(
        Dom::create_body().with_child(
            Dom::create_div()
                .with_ids_and_classes(class("wrap"))
                .with_child(host()),
        ),
    );
    let translation = lw
        .gpu_state_manager
        .get_cache(DomId::ROOT_ID)
        .and_then(|c| c.css_current_transform_values.get(&NodeId::new(1)))
        .copied()
        .expect("harness: the wrapper's transform reaches the GPU cache the raster paints from");
    assert!(
        close(translation.m[3][0], 60.0) && close(translation.m[3][1], 0.0),
        "harness: the wrapper is translated 60px right, got {translation:?}"
    );
    open_session(&mut lw);

    let (at, reported) = static_and_reported(&lw);
    assert!(
        close(reported.origin.x, at.origin.x + 60.0) && close(reported.origin.y, at.origin.y),
        "the caret is painted at {:?} (its layout position {:?} moved 60px right), the IME is told \
         {:?}",
        LogicalPosition::new(at.origin.x + 60.0, at.origin.y),
        at.origin,
        reported.origin
    );
}

/// body(0) > [100px(1), host(2) > text, 900px(3)]: a page taller than the
/// window, scrolled 50px down after the click.
#[test]
fn a_caret_on_a_scrolled_page_is_not_moved_by_the_pages_scrollbar_thumb() {
    let mut lw = layout(
        Dom::create_body()
            .with_child(Dom::create_div().with_ids_and_classes(class("spacer")))
            .with_child(host())
            .with_child(Dom::create_div().with_ids_and_classes(class("tail"))),
    );
    open_session(&mut lw);
    lw.scroll_manager.set_scroll_position(
        DomId::ROOT_ID,
        NodeId::new(0),
        LogicalPosition::new(0.0, 50.0),
        Instant::from(std::time::Instant::now()),
    );
    // The frame's GPU values follow the scroll, the thumb among them.
    lw.refresh_scrollbar_transforms();
    let thumb = lw
        .gpu_state_manager
        .get_cache(DomId::ROOT_ID)
        .and_then(|c| c.current_transform_values.get(&NodeId::new(0)))
        .map_or(0.0, |t| t.m[3][1]);
    assert!(
        thumb > 0.5,
        "harness: the page's thumb moved down with the page (thumb offset {thumb})"
    );

    let (at, reported) = static_and_reported(&lw);
    assert!(
        close(reported.origin.y, at.origin.y - 50.0) && close(reported.origin.x, at.origin.x),
        "the caret is painted 50px above its layout position (the page's scroll), the IME is \
         told {:?} for a layout position of {:?} - the thumb's {thumb}px are no transform of the \
         caret",
        reported.origin,
        at.origin
    );
}
