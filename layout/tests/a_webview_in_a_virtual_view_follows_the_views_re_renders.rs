//! A `<webview>` inside a virtual view follows the view's re-renders.
//!
//! A virtual view's content is a DOM of its own, re-rendered in place - its
//! data changed (`trigger_virtual_view_rerender`), it scrolled to an edge -
//! without a layout of the window. The window's web views were brought in
//! line with the DOMs only at the tail of a full layout, so a web view a
//! re-render added waited for the next unrelated relayout to get its native
//! view, and a changed `src` did not navigate until then.
//!
//! The view's DOM here is `div(0) > [header(1)?] > webview?`, its `src` and
//! whether the header is there set by the test before each re-render.
//!
//! Not compiled by the author (house rule).

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use azul_core::{
    callbacks::{VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn},
    dom::{Dom, DomId, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    refany::RefAny,
    resources::RendererResources,
    styled_dom::StyledDom,
    FastBTreeSet,
};
use azul_css::AzString;
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    managers::webview::{WebViewOp, WebViewPlatform},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// What the view renders: the web view's `src` (no web view without one)
/// and whether a header row comes first.
#[derive(Default)]
struct Content {
    src: Option<String>,
    header: bool,
}

struct ViewData {
    content: Arc<Mutex<Content>>,
}

extern "C" fn render_view(mut data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let (src, header) = data.downcast_ref::<ViewData>().map_or((None, false), |v| {
        let content = v.content.lock().expect("the content");
        (content.src.clone(), content.header)
    });
    let mut dom = Dom::create_div();
    if header {
        dom = dom.with_child(Dom::create_div().with_css("height: 20px;"));
    }
    if let Some(src) = src {
        dom = dom.with_child(
            Dom::create_webview(AzString::from(src.as_str()))
                .with_css("display: block; width: 180px; height: 120px;"),
        );
    }
    let rect = LogicalRect::new(LogicalPosition::zero(), info.bounds.logical_size);
    VirtualViewReturn::with_dom(dom, rect, rect)
}

/// `body(0) > view(1)`: a window with a web view backend, laid out once.
fn window(content: &Arc<Mutex<Content>>) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lw.webviews.set_platform(WebViewPlatform::Backend);
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    let mut dom = Dom::create_body().with_css("margin: 0;").with_child(
        Dom::create_virtual_view(
            RefAny::new(ViewData {
                content: Arc::clone(content),
            }),
            VirtualViewCallback::create(render_view),
        )
        .with_css("width: 200px; height: 200px;"),
    );
    let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    lw
}

/// Re-render the view in place, the way `trigger_virtual_view_rerender`
/// does: no layout of the window.
fn rerender(lw: &mut LayoutWindow) {
    let mut views = FastBTreeSet::new();
    views.insert(NodeId::new(1));
    let mut updates = BTreeMap::new();
    updates.insert(DomId::ROOT_ID, views);
    lw.queue_virtual_view_updates(updates);
    let ws = lw.current_window_state.clone();
    let layouts_before = lw.frame_report.layout_passes;
    let updated = lw.process_pending_virtual_view_updates(
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
    );
    assert_eq!(updated.len(), 1, "harness: the view was re-rendered");
    assert_eq!(
        lw.frame_report.layout_passes, layouts_before,
        "harness: in place, not a layout of the window"
    );
}

fn set(content: &Arc<Mutex<Content>>, src: Option<&str>, header: bool) {
    let mut content = content.lock().expect("the content");
    content.src = src.map(str::to_string);
    content.header = header;
}

/// The view's own dom.
fn view_dom(lw: &LayoutWindow) -> DomId {
    lw.virtual_view_manager
        .get_nested_dom_id(DomId::ROOT_ID, NodeId::new(1))
        .expect("the view rendered a dom")
}

#[test]
fn a_webview_a_views_re_render_adds_gets_its_native_view_at_once() {
    let content = Arc::new(Mutex::new(Content::default()));
    let mut lw = window(&content);
    assert!(lw.webviews.views().is_empty(), "no web view yet");
    let _ = lw.webviews.take_ops();

    set(&content, Some("https://a.example/"), false);
    rerender(&mut lw);
    let views = lw.webviews.views();
    assert_eq!(views.len(), 1, "the re-render's web view is mounted");
    assert_eq!(views[0].node.dom, view_dom(&lw), "in the view's dom");
    let ops = lw.webviews.take_ops();
    assert!(
        ops.iter().any(|op| matches!(
            op,
            WebViewOp::Create { src, .. } if src.as_str() == "https://a.example/"
        )),
        "its native view is created now, not at some later layout: {ops:?}"
    );
}

#[test]
fn a_webview_a_views_re_render_moves_keeps_its_native_view_and_follows_its_src() {
    let content = Arc::new(Mutex::new(Content {
        src: Some("https://a.example/".to_string()),
        header: false,
    }));
    let mut lw = window(&content);
    let id = lw.webviews.views()[0].id;
    let before = lw.webviews.views()[0].node.node.into_crate_internal();
    let _ = lw.webviews.take_ops();

    // A header above it shifts its node; its page changes.
    set(&content, Some("https://b.example/"), true);
    rerender(&mut lw);
    let views = lw.webviews.views();
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].id, id, "the same native view");
    assert_ne!(
        views[0].node.node.into_crate_internal(),
        before,
        "harness: the header moved the web view's node"
    );
    let ops = lw.webviews.take_ops();
    assert!(
        !ops.iter()
            .any(|op| matches!(op, WebViewOp::Create { .. } | WebViewOp::Destroy { .. })),
        "a re-render that keeps the node keeps the view: {ops:?}"
    );
    assert!(
        ops.contains(&WebViewOp::Navigate {
            id,
            url: AzString::from("https://b.example/"),
        }),
        "the changed src navigates it now: {ops:?}"
    );
}

#[test]
fn a_webview_a_views_re_render_drops_is_destroyed() {
    let content = Arc::new(Mutex::new(Content {
        src: Some("https://a.example/".to_string()),
        header: false,
    }));
    let mut lw = window(&content);
    let id = lw.webviews.views()[0].id;
    let _ = lw.webviews.take_ops();

    set(&content, None, false);
    rerender(&mut lw);
    assert!(lw.webviews.views().is_empty());
    assert_eq!(lw.webviews.take_ops(), vec![WebViewOp::Destroy { id }]);
}
