//! A composited web view's page gets the input aimed at it.
//!
//! A native web view (macOS, Windows) is a view of its own: the platform
//! hands it the pointer and the keyboard. A composited one (WPE `WebKit` on
//! Linux) is an image the window draws, so the window routes the input: the
//! pointer when the web view is the topmost node under it (not when
//! something is stacked above it), at the page point the hit test found;
//! the keyboard while the web view has the focus. A press on the page keeps
//! the pointer until its release, as a page's own drag does
//! (`LayoutWindow::route_webview_pointer` / `route_webview_key`, queued as
//! `WebViewOp::Input`).
//!
//! The page: `body(0) > [webview(1) at (12, 12) 200x100, cover(2)]`; the hit
//! tests are pushed by hand (the shell's hit tester is not the subject).
//!
//! Not compiled by the author (house rule).

use std::collections::BTreeMap;

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    events::{KeyModifiers, MouseButton},
    geom::{LogicalPosition, LogicalSize},
    hit_test::{FullHitTest, HitTest, HitTestItem},
    resources::RendererResources,
    spaces::ContentBoxLocal,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_css::AzString;
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    managers::{
        hover::InputPointId,
        webview::{WebViewId, WebViewInput, WebViewOp, WebViewPlatform, WebViewPointer},
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const WEBVIEW: usize = 1;
const COVER: usize = 2;

fn window(platform: WebViewPlatform) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lw.webviews.set_platform(platform);
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    let dom = Dom::create_body()
        .with_css("margin: 0;")
        .with_child(
            Dom::create_webview(AzString::from("https://example.com/")).with_css(
                "display: block; width: 200px; height: 100px; padding: 10px; border: 2px solid \
                 black;",
            ),
        )
        .with_child(Dom::create_div().with_css("width: 50px; height: 50px;"));
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    lw.sync_webview_placements();
    let _ = lw.webviews.take_ops();
    lw
}

fn node(index: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
    }
}

/// The pointer is over `hits` - (node, depth, point in its content box),
/// depth 0 frontmost.
fn hover(lw: &mut LayoutWindow, hits: &[(usize, u32, (f32, f32))]) {
    let mut hit = HitTest::empty();
    for (index, depth, (x, y)) in hits {
        hit.regular_hit_test_nodes.insert(
            NodeId::new(*index),
            HitTestItem {
                point_in_viewport: LogicalPosition::new(*x, *y),
                point_relative_to_item: ContentBoxLocal::new(LogicalPosition::new(*x, *y)),
                is_focusable: true,
                is_virtual_view_hit: None,
                hit_depth: *depth,
            },
        );
    }
    let mut hovered_nodes = BTreeMap::new();
    if !hits.is_empty() {
        hovered_nodes.insert(DomId::ROOT_ID, hit);
    }
    lw.hover_manager.push_hit_test(
        InputPointId::Mouse,
        FullHitTest {
            hovered_nodes,
            focused_node: None.into(),
        },
    );
}

fn the_view(lw: &LayoutWindow) -> WebViewId {
    lw.webviews.views()[0].id
}

fn inputs(lw: &mut LayoutWindow) -> Vec<(WebViewId, WebViewInput)> {
    lw.webviews
        .take_ops()
        .into_iter()
        .filter_map(|op| match op {
            WebViewOp::Input { id, input } => Some((id, input)),
            _ => None,
        })
        .collect()
}

fn at(x: f32, y: f32) -> LogicalPosition {
    LogicalPosition::new(x, y)
}

#[test]
fn the_pointer_over_a_composited_page_reaches_it_at_the_page_point() {
    let mut lw = window(WebViewPlatform::Composited);
    let id = the_view(&lw);
    hover(&mut lw, &[(WEBVIEW, 0, (30.0, 40.0)), (0, 1, (42.0, 52.0))]);
    assert!(lw.route_webview_pointer(at(42.0, 52.0), WebViewPointer::Move));
    assert_eq!(
        inputs(&mut lw),
        vec![(id, WebViewInput::PointerMove { at: at(30.0, 40.0) })]
    );
}

#[test]
fn the_pointer_over_something_stacked_above_the_page_stays_with_the_window() {
    let mut lw = window(WebViewPlatform::Composited);
    hover(&mut lw, &[(COVER, 0, (5.0, 5.0)), (WEBVIEW, 1, (30.0, 40.0))]);
    assert!(!lw.route_webview_pointer(at(42.0, 52.0), WebViewPointer::Move));
    assert_eq!(inputs(&mut lw), Vec::new());
}

#[test]
fn a_press_on_the_page_keeps_the_pointer_until_its_release() {
    let mut lw = window(WebViewPlatform::Composited);
    let id = the_view(&lw);
    hover(&mut lw, &[(WEBVIEW, 0, (30.0, 40.0))]);
    let left = |pressed| WebViewPointer::Button {
        button: MouseButton::Left,
        pressed,
    };
    assert!(lw.route_webview_pointer(at(42.0, 52.0), left(true)));

    // Dragged off the page, to (300, 200): still the page's, at the page
    // point there - the content box starts at (12, 12).
    hover(&mut lw, &[(0, 0, (300.0, 200.0))]);
    assert!(lw.route_webview_pointer(at(300.0, 200.0), WebViewPointer::Move));
    assert!(lw.route_webview_pointer(at(300.0, 200.0), left(false)));
    assert_eq!(
        inputs(&mut lw),
        vec![
            (
                id,
                WebViewInput::PointerButton {
                    at: at(30.0, 40.0),
                    button: MouseButton::Left,
                    pressed: true,
                }
            ),
            (id, WebViewInput::PointerMove { at: at(288.0, 188.0) }),
            (
                id,
                WebViewInput::PointerButton {
                    at: at(288.0, 188.0),
                    button: MouseButton::Left,
                    pressed: false,
                }
            ),
        ]
    );

    // Released: the next move off the page is the window's, and the page
    // hears the pointer leave.
    assert!(!lw.route_webview_pointer(at(300.0, 200.0), WebViewPointer::Move));
    assert_eq!(inputs(&mut lw), vec![(id, WebViewInput::PointerLeave)]);
}

#[test]
fn a_wheel_over_the_page_scrolls_the_page() {
    let mut lw = window(WebViewPlatform::Composited);
    let id = the_view(&lw);
    hover(&mut lw, &[(WEBVIEW, 0, (30.0, 40.0))]);
    assert!(lw.route_webview_pointer(
        at(42.0, 52.0),
        WebViewPointer::Wheel {
            delta: at(0.0, 120.0)
        }
    ));
    assert_eq!(
        inputs(&mut lw),
        vec![(
            id,
            WebViewInput::Wheel {
                at: at(30.0, 40.0),
                delta: at(0.0, 120.0)
            }
        )]
    );
}

#[test]
fn a_native_web_view_gets_no_routed_input() {
    let mut lw = window(WebViewPlatform::Backend);
    hover(&mut lw, &[(WEBVIEW, 0, (30.0, 40.0))]);
    assert!(!lw.route_webview_pointer(at(42.0, 52.0), WebViewPointer::Move));
    lw.focus_manager.set_focused_node(Some(node(WEBVIEW)));
    assert!(!lw.route_webview_key(0x61, 38, true, KeyModifiers::default()));
    assert_eq!(inputs(&mut lw), Vec::new(), "it takes its input itself");
}

#[test]
fn the_keyboard_reaches_a_focused_composited_page() {
    let mut lw = window(WebViewPlatform::Composited);
    let id = the_view(&lw);
    assert!(
        !lw.route_webview_key(0x61, 38, true, KeyModifiers::default()),
        "not focused: the window's"
    );
    lw.focus_manager.set_focused_node(Some(node(WEBVIEW)));
    let shift = KeyModifiers {
        shift: true,
        ..KeyModifiers::default()
    };
    assert!(lw.route_webview_key(0x41, 38, true, shift));
    assert_eq!(
        inputs(&mut lw),
        vec![(
            id,
            WebViewInput::Key {
                native_key: 0x41,
                native_scan: 38,
                pressed: true,
                modifiers: shift,
            }
        )]
    );
}

#[test]
fn a_composited_page_hears_when_it_gets_and_loses_the_focus() {
    let mut lw = window(WebViewPlatform::Composited);
    let id = the_view(&lw);
    lw.focus_manager.set_focused_node(Some(node(WEBVIEW)));
    lw.sync_webview_placements();
    assert_eq!(inputs(&mut lw), vec![(id, WebViewInput::Focus(true))]);
    lw.sync_webview_placements();
    assert_eq!(inputs(&mut lw), Vec::new(), "said once");
    lw.focus_manager.set_focused_node(Some(node(COVER)));
    lw.sync_webview_placements();
    assert_eq!(inputs(&mut lw), vec![(id, WebViewInput::Focus(false))]);
}
