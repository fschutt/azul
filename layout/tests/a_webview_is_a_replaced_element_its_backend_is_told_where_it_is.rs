//! `<webview>` in the engine.
//!
//! The node is a replaced element - 300x150 unless CSS or its `width` /
//! `height` attributes size it - whose display-list item reserves its CONTENT
//! box, and `LayoutWindow::webviews` keys one native view to the node for as
//! long as it is mounted: a rebuild of the same node keeps the view (the page,
//! its scroll and its sign-in state live in it), a changed `src` navigates it,
//! an unmount destroys it. Where the view goes is the display list's answer
//! resolved against the live scroll offsets and the enclosing clips, so a view
//! scrolled out of its box is hidden, not destroyed.
//!
//! A window without a web view backend (a platform that has none yet, a
//! system without the library) shows WHY inside the node's box and tells the
//! app through `WebViewLoadFailed`.
//!
//! Every page is 400x300 without margins.

use azul_core::{
    dom::{
        AttributeNameValue, AttributeType, Dom, DomId, DomNodeId, NodeData, NodeId, NodeType,
    },
    events::EventType,
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
    task::Instant,
    webview::{WebViewCommand, WebViewEvent, WebViewLoadError, WebViewNavigation},
};
use azul_css::AzString;
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    managers::webview::{WebViewId, WebViewOp, WebViewPlacement, WebViewPlatform, WebViewReport},
    solver3::display_list::DisplayListItem,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const SIGN_IN: &str = "https://www.dropbox.com/oauth2/authorize?client_id=x";

/// A window whose shell has a web view backend.
fn window() -> LayoutWindow {
    let mut lw = bare_window();
    lw.webviews.set_platform(WebViewPlatform::Backend);
    lw
}

/// A window as `LayoutWindow::new` leaves it: no backend.
fn bare_window() -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws;
    lw
}

fn lay_out(lw: &mut LayoutWindow, styled: StyledDom) {
    let ws = lw.current_window_state.clone();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
}

fn lay_out_dom(lw: &mut LayoutWindow, dom: Dom) {
    lay_out(lw, StyledDom::create_from_dom(dom));
}

fn node(index: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
    }
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
    LogicalRect::new(LogicalPosition::new(x, y), LogicalSize::new(w, h))
}

fn now() -> Instant {
    Instant::from(std::time::Instant::now())
}

/// `body(0) > webview(1)`, the web view styled by `css`.
fn page(src: &str, css: &str) -> Dom {
    Dom::create_body()
        .with_css("margin: 0;")
        .with_child(Dom::create_webview(AzString::from(src)).with_css(css))
}

fn creates(ops: &[WebViewOp]) -> Vec<(WebViewId, String)> {
    ops.iter()
        .filter_map(|op| match op {
            WebViewOp::Create { id, src, .. } => Some((*id, src.as_str().to_string())),
            _ => None,
        })
        .collect()
}

fn placement_of(ops: &[WebViewOp], of: WebViewId) -> Option<WebViewPlacement> {
    ops.iter().rev().find_map(|op| match op {
        WebViewOp::Place { id, placement } if *id == of => Some(*placement),
        _ => None,
    })
}

#[test]
fn a_webview_without_a_size_is_300_by_150() {
    let mut lw = window();
    lay_out_dom(&mut lw, page(SIGN_IN, ""));
    assert_eq!(
        lw.get_node_size(node(1)),
        Some(LogicalSize::new(300.0, 150.0)),
        "a replaced element without a natural size is 300x150 (CSS Sizing 3, 5.1)"
    );
}

#[test]
fn css_width_and_height_size_a_webview() {
    let mut lw = window();
    lay_out_dom(&mut lw, page(SIGN_IN, "width: 360px; height: 480px;"));
    assert_eq!(
        lw.get_node_size(node(1)),
        Some(LogicalSize::new(360.0, 480.0))
    );
}

/// HTML gives an iframe's `width` / `height` attributes to the dimension
/// properties (15.4.3), below the element's own style.
#[test]
fn the_width_and_height_attributes_size_a_webview_below_its_css() {
    let attr = |name: &str, value: &str| {
        AttributeType::Custom(AttributeNameValue {
            attr_name: AzString::from(name),
            value: AzString::from(value),
        })
    };
    let sized = |css: &str| {
        let mut webview = NodeData::create_webview(AzString::from(SIGN_IN));
        if !css.is_empty() {
            webview.set_css(css);
        }
        Dom::create_body().with_css("margin: 0;").with_child(
            Dom::create_from_data(webview)
                .with_attribute(attr("width", "320"))
                .with_attribute(attr("height", "200")),
        )
    };

    let mut lw = window();
    lay_out_dom(&mut lw, sized(""));
    assert_eq!(
        lw.get_node_size(node(1)),
        Some(LogicalSize::new(320.0, 200.0)),
        "the attributes size the view"
    );

    let mut lw = window();
    lay_out_dom(&mut lw, sized("width: 100px;"));
    assert_eq!(
        lw.get_node_size(node(1)),
        Some(LogicalSize::new(100.0, 200.0)),
        "CSS wins over the attribute it names, the other attribute still applies"
    );
}

/// The item reserves the CONTENT box - inside the border and the padding, as
/// an iframe's page is - and paints nothing itself.
#[test]
fn the_display_list_reserves_the_content_box_of_a_webview() {
    let mut lw = window();
    lay_out_dom(
        &mut lw,
        page(
            SIGN_IN,
            "display: block; width: 300px; height: 150px; padding: 10px; border: 2px solid \
             black;",
        ),
    );
    let items = &lw.layout_results[&DomId::ROOT_ID].display_list.items;
    let reserved: Vec<(NodeId, LogicalRect)> = items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::WebView { node_id, bounds } => Some((*node_id, *bounds.inner())),
            _ => None,
        })
        .collect();
    assert_eq!(
        reserved,
        vec![(NodeId::new(1), rect(12.0, 12.0, 300.0, 150.0))],
        "one item, at the content box"
    );
    assert!(
        items
            .iter()
            .find(|i| matches!(i, DisplayListItem::WebView { .. }))
            .is_some_and(|i| i.visual_bounds().is_none()),
        "the native view is drawn by the platform, never by the raster: no damage of its own"
    );
}

#[test]
fn a_webview_mounts_once_and_survives_a_rebuild_of_the_same_node() {
    let mut lw = window();
    lay_out_dom(&mut lw, page(SIGN_IN, ""));
    let first = lw.webviews.take_ops();
    let created = creates(&first);
    assert_eq!(created.len(), 1, "mounting creates one view: {first:?}");
    let (id, src) = created[0].clone();
    assert_eq!(src, SIGN_IN, "and loads its src");
    assert_eq!(
        lw.webviews.view_at(node(1)).map(|v| v.id),
        Some(id),
        "the view is the node's"
    );

    // The app rebuilds its DOM: the same node, a new arena.
    lay_out_dom(&mut lw, page(SIGN_IN, ""));
    let second = lw.webviews.take_ops();
    assert!(
        !second
            .iter()
            .any(|op| matches!(op, WebViewOp::Create { .. } | WebViewOp::Destroy { .. })),
        "a rebuild of the same node keeps its view: {second:?}"
    );
    assert_eq!(lw.webviews.view_at(node(1)).map(|v| v.id), Some(id));
}

#[test]
fn a_changed_src_navigates_the_view_the_node_already_has() {
    let mut lw = window();
    lay_out_dom(&mut lw, page(SIGN_IN, ""));
    let id = creates(&lw.webviews.take_ops())[0].0;

    lay_out_dom(&mut lw, page("https://example.com/next", ""));
    assert_eq!(
        lw.webviews.take_ops(),
        vec![WebViewOp::Navigate {
            id,
            url: AzString::from("https://example.com/next"),
        }],
        "the same view goes to the new page"
    );
}

#[test]
fn an_unmounted_webview_is_destroyed() {
    let mut lw = window();
    lay_out_dom(&mut lw, page(SIGN_IN, ""));
    let id = creates(&lw.webviews.take_ops())[0].0;

    lay_out_dom(
        &mut lw,
        Dom::create_body()
            .with_css("margin: 0;")
            .with_child(Dom::create_div()),
    );
    assert_eq!(lw.webviews.take_ops(), vec![WebViewOp::Destroy { id }]);
    assert!(lw.webviews.views().is_empty());
}

#[test]
fn the_backend_hears_where_a_webview_is_after_a_layout() {
    let mut lw = window();
    lay_out_dom(
        &mut lw,
        page(
            SIGN_IN,
            "display: block; width: 300px; height: 150px; padding: 10px; border: 2px solid \
             black;",
        ),
    );
    lw.sync_webview_placements();
    let ops = lw.webviews.take_ops();
    let id = creates(&ops)[0].0;
    assert_eq!(
        placement_of(&ops, id),
        Some(WebViewPlacement {
            rect: rect(12.0, 12.0, 300.0, 150.0),
            clip: rect(12.0, 12.0, 300.0, 150.0),
            visible: true,
        }),
        "the content box, wholly visible: {ops:?}"
    );

    lw.sync_webview_placements();
    assert_eq!(
        lw.webviews.take_ops(),
        Vec::new(),
        "an unchanged placement is not sent again"
    );
}

/// `body(0) > box(1, scrolls) > [spacer(2), webview(3)]`: the view sits below
/// the box's bottom edge until the box is scrolled.
#[test]
fn a_webview_scrolled_out_of_its_box_is_hidden_not_destroyed() {
    let mut lw = window();
    lay_out_dom(
        &mut lw,
        Dom::create_body().with_css("margin: 0;").with_child(
            Dom::create_div()
                .with_css("overflow: auto; width: 400px; height: 200px;")
                .with_child(Dom::create_div().with_css("height: 400px;"))
                .with_child(
                    Dom::create_webview(AzString::from(SIGN_IN))
                        .with_css("display: block; width: 300px; height: 150px;"),
                ),
        ),
    );
    lw.sync_webview_placements();
    let ops = lw.webviews.take_ops();
    let id = creates(&ops)[0].0;
    let hidden = placement_of(&ops, id).expect("placed");
    assert!(
        !hidden.visible,
        "below the box's bottom edge nothing of it shows: {hidden:?}"
    );
    assert_eq!(hidden.rect, rect(0.0, 400.0, 300.0, 150.0));

    assert!(
        lw.scroll_manager
            .get_scroll_state(DomId::ROOT_ID, NodeId::new(1))
            .is_some(),
        "harness: the box scrolls"
    );
    lw.scroll_manager.set_scroll_position(
        DomId::ROOT_ID,
        NodeId::new(1),
        LogicalPosition::new(0.0, 300.0),
        now(),
    );
    lw.sync_webview_placements();
    let ops = lw.webviews.take_ops();
    assert!(
        !ops.iter()
            .any(|op| matches!(op, WebViewOp::Create { .. } | WebViewOp::Destroy { .. })),
        "scrolling never recreates the view: {ops:?}"
    );
    assert_eq!(
        placement_of(&ops, id),
        Some(WebViewPlacement {
            rect: rect(0.0, 100.0, 300.0, 150.0),
            clip: rect(0.0, 100.0, 300.0, 100.0),
            visible: true,
        }),
        "scrolled by 300px its top 100px show, clipped by the box"
    );
}

#[test]
fn a_command_from_a_callback_reaches_the_backend() {
    let mut lw = window();
    lay_out_dom(&mut lw, page(SIGN_IN, ""));
    let id = creates(&lw.webviews.take_ops())[0].0;

    for (command, op) in [
        (
            WebViewCommand::Navigate(AzString::from("https://example.com/b")),
            WebViewOp::Navigate {
                id,
                url: AzString::from("https://example.com/b"),
            },
        ),
        (WebViewCommand::Reload, WebViewOp::Reload { id }),
        (WebViewCommand::GoBack, WebViewOp::GoBack { id }),
    ] {
        assert!(lw.webviews.queue_command(node(1), &command));
        assert_eq!(lw.webviews.take_ops(), vec![op]);
    }
    assert!(
        !lw.webviews.queue_command(node(0), &WebViewCommand::Reload),
        "the body is no web view"
    );
}

fn report(id: WebViewId, request: u64, event: WebViewEvent) -> WebViewReport {
    WebViewReport { id, request, event }
}

/// The decision a backend waits for: the app's `prevent_default` cancels,
/// anything else allows - and either is recorded on the view.
#[test]
fn a_navigation_the_app_cancels_does_not_go_ahead() {
    let mut lw = window();
    lay_out_dom(&mut lw, page(SIGN_IN, ""));
    let id = creates(&lw.webviews.take_ops())[0].0;

    let to_the_page = report(
        id,
        7,
        WebViewEvent::NavigationRequested(WebViewNavigation {
            url: AzString::from(SIGN_IN),
            is_redirect: false,
        }),
    );
    let event = lw
        .webviews
        .begin_report(&to_the_page, &now())
        .expect("the app is asked");
    assert_eq!(event.event_type, EventType::WebViewNavigationRequested);
    assert_eq!(event.target, node(1), "at the web view's node");
    assert_eq!(
        lw.webviews.event_of(node(1)),
        Some(&to_the_page.event),
        "what the callback reads"
    );
    assert_eq!(lw.webviews.finish_report(&to_the_page, true, false), Some(true));

    let redirect = report(
        id,
        8,
        WebViewEvent::NavigationRequested(WebViewNavigation {
            url: AzString::from("http://127.0.0.1:53682/callback?code=abc"),
            is_redirect: true,
        }),
    );
    assert!(lw.webviews.begin_report(&redirect, &now()).is_some());
    assert_eq!(
        lw.webviews.finish_report(&redirect, true, true),
        Some(false),
        "prevent_default cancels"
    );

    let view = lw.webviews.view_at(node(1)).expect("the view");
    assert_eq!(view.url.as_str(), SIGN_IN, "the cancelled redirect never loaded");
    let log: Vec<(String, bool, bool)> = view
        .navigations
        .iter()
        .map(|n| (n.url.as_str().to_string(), n.is_redirect, n.allowed))
        .collect();
    assert_eq!(
        log,
        vec![
            (SIGN_IN.to_string(), false, true),
            (
                "http://127.0.0.1:53682/callback?code=abc".to_string(),
                true,
                false
            ),
        ]
    );

    let finished = report(id, 0, WebViewEvent::LoadFinished(AzString::from(SIGN_IN)));
    assert!(lw.webviews.begin_report(&finished, &now()).is_some());
    assert_eq!(lw.webviews.finish_report(&finished, true, false), None);
    let title = report(id, 0, WebViewEvent::TitleChanged(AzString::from("Sign in")));
    assert!(lw.webviews.begin_report(&title, &now()).is_some());
    let _ = lw.webviews.finish_report(&title, true, false);
    let view = lw.webviews.view_at(node(1)).expect("the view");
    assert!(!view.loading);
    assert_eq!(view.title.as_str(), "Sign in");
}

#[test]
fn a_file_url_is_refused_before_the_app_is_asked() {
    let mut lw = window();
    lay_out_dom(&mut lw, page(SIGN_IN, ""));
    let id = creates(&lw.webviews.take_ops())[0].0;
    let local = report(
        id,
        3,
        WebViewEvent::NavigationRequested(WebViewNavigation {
            url: AzString::from("file:///etc/passwd"),
            is_redirect: false,
        }),
    );
    assert!(
        lw.webviews.begin_report(&local, &now()).is_none(),
        "no callback runs for a file:// page"
    );
    assert_eq!(lw.webviews.finish_report(&local, false, false), Some(false));
}

#[test]
fn a_window_without_a_web_view_backend_shows_why_and_tells_the_app() {
    let mut lw = bare_window();
    let reason = lw
        .webviews
        .unavailable_reason()
        .expect("no backend: a reason");
    assert!(!reason.as_str().is_empty());

    let styled = lw.style_user_dom(page(SIGN_IN, ""));
    let texts: Vec<String> = styled
        .node_data
        .as_ref()
        .iter()
        .filter_map(|nd| nd.get_node_type().get_text())
        .map(|t| t.as_str().to_string())
        .collect();
    assert!(
        texts.iter().any(|t| t.contains(reason.as_str())),
        "the web view's box says why it shows no page: {texts:?}"
    );
    assert!(
        matches!(
            styled.node_data.as_ref()[1].get_node_type(),
            NodeType::WebView(_)
        ),
        "the web view keeps its place"
    );

    lay_out(&mut lw, styled);
    assert!(
        lw.webviews.take_ops().is_empty(),
        "nothing for a backend that does not exist"
    );
    let failed = lw
        .pending_lifecycle_events
        .iter()
        .find(|e| e.event_type == EventType::WebViewLoadFailed)
        .expect("the app hears it");
    assert_eq!(failed.target, node(1));
    assert_eq!(
        lw.webviews.event_of(node(1)),
        Some(&WebViewEvent::LoadFailed(WebViewLoadError {
            url: AzString::from(SIGN_IN),
            reason,
        }))
    );
}

/// A probe names the missing piece (`libWPEWebKit-2.0.so.1`, ...); it runs
/// only once a web view is mounted - an app without one loads nothing.
#[test]
fn a_platform_probe_runs_only_for_a_window_with_a_web_view() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static PROBED: AtomicUsize = AtomicUsize::new(0);
    fn probe() -> AzString {
        PROBED.fetch_add(1, Ordering::SeqCst);
        AzString::from("libexample-webview.so.1 not found")
    }

    let mut lw = bare_window();
    lw.webviews.set_platform(WebViewPlatform::Probe(probe));
    let styled = lw.style_user_dom(Dom::create_body().with_child(Dom::create_div()));
    lay_out(&mut lw, styled);
    assert_eq!(
        PROBED.load(Ordering::SeqCst),
        0,
        "no web view, no probe (nothing is loaded)"
    );

    let styled = lw.style_user_dom(page(SIGN_IN, ""));
    lay_out(&mut lw, styled);
    assert!(PROBED.load(Ordering::SeqCst) >= 1);
    assert!(matches!(
        lw.webviews.event_of(node(1)),
        Some(WebViewEvent::LoadFailed(e)) if e.reason.as_str() == "libexample-webview.so.1 not found"
    ));
}
