//! A relayout of an unchanged host keeps its VirtualViews: no callback runs
//! again, no child DOM is laid out cold.
//!
//! The layout funnel cleared every child result and reset every view's
//! invocation flags on EVERY pass, a relayout of the same DOM included (an
//! animation frame, a hover restyle, a resize), so every view's callback -
//! user code - ran again and its child DOM was styled and laid out from
//! scratch, every frame of every animation (AzWidgets: three views, ~1.3 ms
//! of each knob frame; a document editor's page views or a map's tiles make
//! it the dominant per-frame cost). The views' own lifecycle
//! (`check_reinvoke`: the box grew, the user scrolled to an edge) was never
//! consulted.
//!
//! A view is kept when its host node still carries exactly the node it was
//! last invoked for - the same callback and the same dataset instance - and
//! its box has the size its child was laid out for. A new DOM (a rebuild, a
//! new dataset) and a box of another size re-render it as before.

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
}

/// The view's content; counts its invocations.
extern "C" fn render_view(mut data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    if let Some(view) = data.downcast_ref::<ViewData>() {
        view.calls.fetch_add(1, Ordering::SeqCst);
    }
    let size = info.bounds.logical_size;
    let dom = Dom::create_div_with_text("text in the view").with_css("width: 100%; height: 20px;");
    let rect = LogicalRect::new(LogicalPosition::zero(), size);
    VirtualViewReturn::with_dom(dom, rect, rect)
}

/// A host page holding one view, `view_css` sizing its box.
fn host_page(calls: &Arc<AtomicUsize>, view_css: &str) -> StyledDom {
    let mut dom = Dom::create_body()
        .with_child(Dom::create_div_with_text("text in the host"))
        .with_child(
            Dom::create_virtual_view(
                RefAny::new(ViewData {
                    calls: Arc::clone(calls),
                }),
                VirtualViewCallback::create(render_view),
            )
            .with_css(view_css),
        );
    StyledDom::create(&mut dom, azul_css::css::Css::empty())
}

fn window(width: f32) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(width, 400.0);
    lw.current_window_state = ws;
    lw
}

fn lay_out(lw: &mut LayoutWindow, styled_dom: StyledDom) {
    let window_state = lw.current_window_state.clone();
    lw.layout_and_generate_display_list(
        styled_dom,
        &window_state,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the host lays out");
}

/// Lay the host out again from its retained DOM, as an animation frame does.
fn relayout(lw: &mut LayoutWindow) {
    let result = lw.layout_results.remove(&DomId::ROOT_ID).expect("laid out");
    lay_out(lw, result.styled_dom);
}

fn child_doms(lw: &LayoutWindow) -> Vec<DomId> {
    lw.layout_results
        .keys()
        .copied()
        .filter(|d| *d != DomId::ROOT_ID)
        .collect()
}

#[test]
fn a_relayout_of_an_unchanged_host_runs_no_view_callback() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut lw = window(800.0);
    lay_out(&mut lw, host_page(&calls, "width: 200px; height: 20px;"));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "harness: the first layout renders the view once"
    );
    let children = child_doms(&lw);
    assert_eq!(
        children.len(),
        1,
        "harness: the view's child DOM is laid out"
    );

    for frame in 0..3 {
        relayout(&mut lw);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "relayout {frame} of the unchanged host ran the view's callback again - the view, \
             its box and its dataset are what they were"
        );
        assert_eq!(
            child_doms(&lw),
            children,
            "relayout {frame}: the view's child DOM must stay laid out, under the same id"
        );
    }
}

#[test]
fn a_view_whose_box_changed_size_renders_again() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut lw = window(800.0);
    lay_out(&mut lw, host_page(&calls, "width: 50%; height: 20px;"));
    assert_eq!(calls.load(Ordering::SeqCst), 1, "harness: one render");

    // A narrower window: the view's box shrinks from 400 to 300 px. Its child
    // was laid out for 400 px - it renders again for the box it has now.
    lw.current_window_state.size.dimensions = LogicalSize::new(600.0, 400.0);
    relayout(&mut lw);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "the view's box changed size - its callback must run for the new box"
    );
}

#[test]
fn a_relayout_of_a_new_host_dom_renders_its_view() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut lw = window(800.0);
    lay_out(&mut lw, host_page(&calls, "width: 200px; height: 20px;"));
    assert_eq!(calls.load(Ordering::SeqCst), 1, "harness: one render");

    // The same shape, but a NEW view node (a new dataset instance) handed in
    // through the relayout entry: not the view that was rendered.
    let _ = lw.layout_results.remove(&DomId::ROOT_ID);
    lay_out(&mut lw, host_page(&calls, "width: 200px; height: 20px;"));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "a host node carrying another dataset is another view - its callback must run"
    );
}
