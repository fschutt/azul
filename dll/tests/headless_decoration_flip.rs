//! A decoration flip alone rebuilds the window's DOM.
//!
//! The window asks the compositor for server-side decorations; the compositor
//! answers with the other mode (KWin grants `client_side` where we asked for
//! `server_side`). The shell then flips the window to frameless + CSD and asks
//! for a regeneration — and the CSD titlebar has to be in the tree when that
//! regeneration returns.
//!
//! It was not. `regenerate_layout`'s pre-cascade skip compares a fingerprint of
//! the *app's* DOM against the previous one, and the app's DOM is byte-identical
//! across a decoration flip — the flip lives in the window flags, which nothing
//! downstream of the layout callback reads. So the skip fired, the titlebar was
//! never injected, and it appeared only at the next app-driven rebuild: a 14-node
//! prepend that shifts every NodeId, mass-unmatches the reconciler and drops
//! focus, long after the window was up.
//!
//! Headless runs the same `shell2::common::layout::regenerate_layout` as every
//! desktop backend, so the skip — and this regression — is shared by all of them.

use std::{cell::RefCell, sync::Arc};

use azul::desktop::shell2::{common::event::PlatformWindow, headless::HeadlessWindow};
use azul_core::{
    callbacks::{LayoutCallback, LayoutCallbackInfo},
    dom::{Dom, DomId},
    icon::{IconProviderHandle, SharedIconProvider},
    refany::RefAny,
    resources::AppConfig,
    window::WindowDecorations,
};
use azul_layout::window_state::WindowCreateOptions;
use rust_fontconfig::FcFontCache;

/// The class the CSD titlebar's root carries
/// (`azul_layout::widgets::titlebar::Titlebar::dom_with_buttons`).
const CSD_TITLEBAR_CLASS: &str = "csd-titlebar";

struct AppState;

/// A DOM that does NOT change between passes — exactly the case the pre-cascade
/// skip is built for, and exactly the case that hid this bug.
extern "C" fn constant_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    Dom::create_div().with_child(Dom::create_div())
}

fn make_window(decorations: WindowDecorations) -> HeadlessWindow {
    let fc_cache = Arc::new(FcFontCache::default());
    let app_data = Arc::new(RefCell::new(RefAny::new(AppState)));
    let icon_provider = SharedIconProvider::from_handle(IconProviderHandle::default());

    let mut options = WindowCreateOptions::default();
    options.window_state.size.dimensions = azul_core::geom::LogicalSize {
        width: 800.0,
        height: 600.0,
    };
    options.window_state.flags.decorations = decorations;
    options.window_state.flags.has_decorations = true;
    let cb: extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom = constant_layout;
    options.window_state.layout_callback = LayoutCallback::create(cb);

    HeadlessWindow::new(
        options,
        app_data,
        azul::desktop::shell2::common::event::SharedUndoManager::new(),
        AppConfig::default(),
        icon_provider,
        fc_cache,
        None,
    )
    .expect("HeadlessWindow construction must succeed")
}

/// Node count + "is there a titlebar in it" for the window's root DOM.
fn root_dom_shape(window: &HeadlessWindow) -> (usize, bool) {
    let layout_window = window
        .common
        .layout_window
        .as_ref()
        .expect("the window has a LayoutWindow after regenerate_layout");
    let styled_dom = &layout_window
        .layout_results
        .get(&DomId::ROOT_ID)
        .expect("the root DOM was laid out")
        .styled_dom;
    let nodes = styled_dom.node_data.as_ref();
    let has_titlebar = nodes.iter().any(|nd| nd.has_class(CSD_TITLEBAR_CLASS));
    (nodes.len(), has_titlebar)
}

#[test]
fn a_refused_server_side_decoration_grows_the_titlebar_in_the_same_pass() {
    // The window went up expecting the compositor to draw its frame, so its
    // first DOM is the app's and nothing else.
    let mut window = make_window(WindowDecorations::Normal);
    window.regenerate_layout().expect("initial regenerate_layout");

    let (nodes_before, titlebar_before) = root_dom_shape(&window);
    assert!(
        !titlebar_before,
        "a server-decorated window must not carry a CSD titlebar"
    );

    // The compositor answers `client_side`. Nothing about the APP changed —
    // `constant_layout` returns the identical DOM — so the only evidence of the
    // flip is the window flags.
    window.simulate_decoration_change(WindowDecorations::None, true);
    window
        .regenerate_layout()
        .expect("post-decoration-change regenerate_layout");

    let (nodes_after, titlebar_after) = root_dom_shape(&window);
    assert!(
        titlebar_after,
        "the decoration flip alone must rebuild the DOM with the CSD titlebar in it — a window \
         whose compositor refuses server-side decorations is otherwise a bare, uncloseable \
         rectangle until some unrelated rebuild happens to reshape the tree"
    );
    assert!(
        nodes_after > nodes_before,
        "the injected titlebar adds nodes ({nodes_before} -> {nodes_after})"
    );
}

#[test]
fn dropping_the_decoration_flip_drops_the_titlebar_again() {
    // The mirror image: a CSD window handed real server-side decorations must
    // lose its software titlebar, or it draws a second, fake one under the
    // compositor's.
    let mut window = make_window(WindowDecorations::None);
    window.regenerate_layout().expect("initial regenerate_layout");

    let (_, titlebar_before) = root_dom_shape(&window);
    assert!(
        titlebar_before,
        "frameless + has_decorations is the full-CSD case: the titlebar is ours to draw"
    );

    window.simulate_decoration_change(WindowDecorations::Normal, true);
    window
        .regenerate_layout()
        .expect("post-decoration-change regenerate_layout");

    let (_, titlebar_after) = root_dom_shape(&window);
    assert!(
        !titlebar_after,
        "the compositor draws the frame now — ours must go, in this pass"
    );
}

#[test]
fn an_unchanged_decoration_mode_still_takes_the_skip() {
    // The guard must cost nothing in the steady state: an identical DOM with
    // unchanged flags still reports LayoutUnchanged, i.e. the pre-cascade skip
    // is not disarmed by merely having the precheck.
    use azul::desktop::shell2::common::layout::LayoutRegenerateResult;

    let mut window = make_window(WindowDecorations::Normal);
    // Warm up the way `leak_regression.rs` does, so the caches the skip needs
    // (fingerprints, the retained StyledDom) are populated and the measured
    // pass is a genuine steady-state one.
    for _ in 0..3 {
        window.regenerate_layout().expect("warmup regenerate_layout");
    }

    let result = window
        .regenerate_layout_inner()
        .expect("steady-state regenerate_layout");
    assert_eq!(
        result,
        LayoutRegenerateResult::LayoutUnchanged,
        "an identical DOM under unchanged decoration flags must still skip the rebuild"
    );
}
