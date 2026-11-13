//! A `VirtualView`'s child DOM lays out warm when the view renders again.
//!
//! A view's callback runs again on every new generation of its host (and when
//! it scrolls past what it materialized), and returns its child DOM anew. That
//! DOM was laid out against an empty layout cache every time, so text that had
//! not changed was broken into lines again and every box was measured again:
//! AzPdf's page view paid a cold layout of ~3600 nodes (~110 ms) for every
//! re-render. The child DOM keeps a layout cache of its own between passes, as
//! the root does, and an unchanged child lays out from it - or, when the
//! callback built the very DOM it built last, keeps its laid-out child whole.

use azul_core::{
    callbacks::{VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn},
    dom::{Dom, DomId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    refany::RefAny,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    probe::{Event, Probe},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

struct ViewData;

/// How often the view's callback ran.
static RENDERS: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

/// The view's content: a column of paragraphs, the same on every render.
extern "C" fn render_view(_data: RefAny, _info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    RENDERS.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
    let mut column = Dom::create_div().with_css("width: 300px;");
    for i in 0..12 {
        column = column.with_child(Dom::create_p().with_child(
            Dom::create_text_do_not_use_without_block_level_wrapper(format!(
                "paragraph {i} of the page in the view"
            )),
        ));
    }
    let rect = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(300.0, 400.0));
    VirtualViewReturn::with_dom(column, rect, rect)
}

/// A host with no text of its own around one view: every line broken in a pass
/// is the view's.
fn host_page() -> StyledDom {
    let mut dom = Dom::create_body().with_child(
        Dom::create_virtual_view(
            RefAny::new(ViewData),
            VirtualViewCallback::create(render_view),
        )
        .with_css("width: 300px; height: 400px;"),
    );
    StyledDom::create(&mut dom, azul_css::css::Css::empty())
}

/// A new generation of the host: its view renders again.
fn lay_out_new_generation(lw: &mut LayoutWindow) {
    let window_state = lw.current_window_state.clone();
    lw.layout_and_generate_display_list(
        host_page(),
        &window_state,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the host lays out");
}

fn count(events: &[Event], name: &str) -> usize {
    events.iter().filter(|e| e.name == name).count()
}

#[test]
fn a_virtual_views_child_dom_lays_out_warm() {
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = ws;
    lay_out_new_generation(&mut lw);
    lay_out_new_generation(&mut lw);
    assert!(
        lw.layout_results.keys().any(|d| *d != DomId::ROOT_ID),
        "harness: the view's child DOM was laid out"
    );

    let _serialised = crate::probe_lock();
    if !Probe::enabled() {
        eprintln!("[child dom warm] the probe is compiled out: nothing to count");
        return;
    }
    Probe::set_recording(true);
    let _ = Probe::drain();
    let renders_before = RENDERS.load(core::sync::atomic::Ordering::SeqCst);
    lay_out_new_generation(&mut lw);
    let events = Probe::drain();
    let rendered = RENDERS.load(core::sync::atomic::Ordering::SeqCst) > renders_before;
    let ambient = azul_core::profile::cpu_enabled()
        || azul_core::profile::memory_enabled()
        || azul_core::profile::heap_enabled();
    Probe::set_recording(ambient);
    let _ = Probe::drain();

    assert!(
        rendered,
        "harness: the new generation ran the view's callback again"
    );
    assert!(
        lw.layout_results.keys().any(|d| *d != DomId::ROOT_ID),
        "the view's child DOM is still laid out after it rendered again"
    );
    let flows = count(&events, "text_layout_flow");
    assert_eq!(
        flows, 0,
        "the view rendered the same twelve paragraphs again and they were broken into lines \
         {flows} times: its child DOM was laid out cold"
    );
}
