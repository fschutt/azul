//! A rebuild whose view shows the same keeps the view's child.
//!
//! A new generation of the host (the app rebuilt its window: a ribbon
//! opened, a tab switched) runs every view's callback again, as the view
//! may show new app state. When the callback builds the same DOM as last
//! time, the child it was styled and laid out into is still right: it is
//! kept, with the fresh build's callbacks and data, instead of being styled
//! and laid out from scratch (AzDrive's folder view: half of each hover
//! frame). A view that builds another DOM is styled anew.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use azul_core::{
    callbacks::{VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn},
    dom::{Dom, DomId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    refany::RefAny,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

struct ViewData {
    calls: Arc<AtomicUsize>,
    rows: usize,
}

/// `rows` lines of text; counts its invocations.
extern "C" fn render_view(mut data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let rows = data.downcast_ref::<ViewData>().map_or(0, |view| {
        view.calls.fetch_add(1, Ordering::SeqCst);
        view.rows
    });
    let mut dom = Dom::create_div();
    for i in 0..rows {
        dom = dom.with_child(
            Dom::create_div_with_text(format!("row {i}")).with_css("height: 20px;"),
        );
    }
    let rect = LogicalRect::new(LogicalPosition::zero(), info.bounds.logical_size);
    VirtualViewReturn::with_dom(dom, rect, rect)
}

/// A new build of the host: a new dataset, a view of `rows` rows.
fn host_page(calls: &Arc<AtomicUsize>, rows: usize) -> StyledDom {
    let mut dom = Dom::create_body()
        .with_child(Dom::create_div_with_text("text in the host"))
        .with_child(
            Dom::create_virtual_view(
                RefAny::new(ViewData {
                    calls: Arc::clone(calls),
                    rows,
                }),
                VirtualViewCallback::create(render_view),
            )
            .with_css("width: 300px; height: 200px;"),
        );
    StyledDom::create(&mut dom, azul_css::css::Css::empty())
}

fn rebuild(lw: &mut LayoutWindow, styled_dom: StyledDom) {
    let ws = lw.current_window_state.clone();
    lw.layout_new_generation(
        styled_dom,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the host lays out");
}

/// The view's child DOM and the address of its layout tree.
fn child(lw: &LayoutWindow) -> (DomId, usize) {
    let (dom, result) = lw
        .layout_results
        .iter()
        .find(|(d, _)| **d != DomId::ROOT_ID)
        .expect("the view's child is laid out");
    (*dom, Arc::as_ptr(&result.layout_tree) as usize)
}

fn window() -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(800.0, 400.0);
    lw.current_window_state = ws;
    lw
}

#[test]
fn a_view_that_builds_the_same_dom_keeps_its_laid_out_child() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut lw = window();
    rebuild(&mut lw, host_page(&calls, 5));
    let first = child(&lw);
    for _ in 0..3 {
        rebuild(&mut lw, host_page(&calls, 5));
        assert_eq!(
            child(&lw),
            first,
            "the same rows: the child stays as it was styled and laid out"
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 4, "every rebuild asks the view");
}

#[test]
fn a_view_that_builds_another_dom_is_styled_anew() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut lw = window();
    rebuild(&mut lw, host_page(&calls, 5));
    let first = child(&lw);
    rebuild(&mut lw, host_page(&calls, 6));
    let second = child(&lw);
    assert_ne!(second.1, first.1, "another row: the child is laid out again");
    let rows = lw.layout_results[&second.0].styled_dom.node_data.as_ref().len();
    assert!(rows > 6, "the sixth row is in the child: {rows} nodes");
}
