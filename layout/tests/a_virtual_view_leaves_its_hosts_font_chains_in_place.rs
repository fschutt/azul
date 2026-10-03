//! A `VirtualView`'s layout leaves its host's font chains in place.
//!
//! The font manager keeps ONE chain cache and the signature of the font
//! stacks it was resolved for (`font_chain_cache`,
//! `last_resolved_font_stacks_sig`); a pass whose DOM has the same stacks
//! skips the resolver and its DOM scan. A view's child DOM is laid out INSIDE
//! its host's pass, after the host's text, and replaced both with its own, so
//! the host's next pass never matched: AzWidgets (three views) re-resolved its
//! 3472-node page's fonts on every relayout - every switch-knob frame
//! (`font_chain_resolve` 2.2 ms of a tick) - and between passes the window's
//! chain cache described the last view laid out, not the window.
//!
//! Not compiled by the author (house rule). Expected RED before the fix: the
//! host's chains are gone after a pass, and a relayout resolves twice.

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

/// The view's content: text in a family the host does not use.
extern "C" fn render_view(_data: RefAny, _info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let dom = Dom::create_div()
        .with_css("font-family: monospace; width: 200px; height: 20px;")
        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
            "text in the view",
        ));
    let rect = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(200.0, 20.0));
    VirtualViewReturn::with_dom(dom, rect, rect)
}

/// A host page in `serif` holding a view.
fn host_page() -> StyledDom {
    let mut dom = Dom::create_body()
        .with_css("font-family: serif;")
        .with_child(Dom::create_p().with_child(
            Dom::create_text_do_not_use_without_block_level_wrapper("text in the host"),
        ))
        .with_child(
            Dom::create_virtual_view(
                RefAny::new(ViewData),
                VirtualViewCallback::create(render_view),
            )
            .with_css("width: 200px; height: 20px;"),
        );
    StyledDom::create(&mut dom, azul_css::css::Css::empty())
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

fn holds_a_chain_for(lw: &LayoutWindow, family: &str) -> bool {
    lw.font_manager.font_chain_cache.keys().any(|k| {
        k.font_families
            .iter()
            .any(|f| f.eq_ignore_ascii_case(family))
    })
}

fn count(events: &[Event], name: &str) -> usize {
    events.iter().filter(|e| e.name == name).count()
}

#[test]
fn a_virtual_view_leaves_its_hosts_font_chains_in_place() {
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = ws;
    lay_out(&mut lw, host_page());
    relayout(&mut lw);

    assert!(
        lw.layout_results.keys().any(|d| *d != DomId::ROOT_ID),
        "harness: the view's child DOM was laid out"
    );
    assert!(
        holds_a_chain_for(&lw, "serif"),
        "after a pass the window's font chains must still hold the host's (serif) - a view laid \
         out inside the host's pass replaced them with its own: {:?}",
        lw.font_manager
            .font_chain_cache
            .keys()
            .map(|k| k.font_families.clone())
            .collect::<Vec<_>>()
    );

    // A relayout of the unchanged host resolves no host fonts: only the view,
    // whose child DOM every pass builds anew, resolves its own (one
    // `font_load_missing` per resolution).
    let _serialised = crate::probe_lock();
    if !Probe::enabled() {
        eprintln!("[font chains] the probe is compiled out: nothing to count");
        return;
    }
    Probe::set_recording(true);
    let _ = Probe::drain();
    relayout(&mut lw);
    let events = Probe::drain();
    let ambient = azul_core::profile::cpu_enabled()
        || azul_core::profile::memory_enabled()
        || azul_core::profile::heap_enabled();
    Probe::set_recording(ambient);
    let _ = Probe::drain();

    let resolutions = count(&events, "font_load_missing");
    assert!(
        resolutions <= 1,
        "a relayout of an unchanged host with one view resolved fonts {resolutions} times - the \
         host's own stacks were resolved again because the view's pass overwrote their signature"
    );
}
