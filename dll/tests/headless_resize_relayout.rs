//! End-to-end check that a window resize re-invokes the user's
//! `layout()` callback and that `LayoutCallbackInfo::relayout_reason()`
//! correctly reports `Initial` on the first call and `Resize` after
//! `simulate_resize()`.
//!
//! This is the headless analogue of dragging the window edge across a
//! CSS breakpoint. The user's layout callback may emit a structurally
//! different DOM in narrow vs wide layouts (hamburger nav vs sidebar);
//! the framework must re-run `layout()` so the new shape can take
//! effect, and the callback must be able to tell *why* it was called
//! (a Resize relayout can skip work that doesn't depend on the window
//! size, like analytics fetches).

use std::cell::RefCell;
use std::sync::Arc;

use azul_core::callbacks::{LayoutCallback, LayoutCallbackInfo, RelayoutReason};
use azul_core::dom::Dom;
use azul_core::icon::{IconProviderHandle, SharedIconProvider};
use azul_core::refany::RefAny;
use azul_core::resources::AppConfig;
use azul_layout::window_state::WindowCreateOptions;
use rust_fontconfig::FcFontCache;

use azul::desktop::shell2::headless::HeadlessWindow;
use azul::desktop::shell2::common::event::PlatformWindow;

/// What each `layout()` invocation observed: the relayout reason and the
/// window size at call time. The recording callback writes into a Vec
/// owned by the app's RefAny so each test sees its own observations and
/// no global mutex can leak panic poisoning across tests.
#[derive(Debug, Clone, PartialEq)]
struct LayoutObservation {
    reason: RelayoutReason,
    width: f32,
    height: f32,
}

#[derive(Default)]
struct RecordingState {
    observations: Vec<LayoutObservation>,
}

extern "C" fn recording_layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    if let Some(mut state) = data.downcast_mut::<RecordingState>() {
        state.observations.push(LayoutObservation {
            reason: info.relayout_reason(),
            width: info.window_size.dimensions.width,
            height: info.window_size.dimensions.height,
        });
    }
    // Build a trivial DOM. The layout result is irrelevant; we only
    // care that the callback ran and saw the expected size + reason.
    Dom::create_div()
}

fn read_observations(window: &HeadlessWindow) -> Vec<LayoutObservation> {
    let app_data = window.get_app_data();
    let mut app_data_ref = app_data.borrow_mut();
    app_data_ref
        .downcast_mut::<RecordingState>()
        .map(|s| s.observations.clone())
        .unwrap_or_default()
}

fn make_window() -> HeadlessWindow {

    let fc_cache = Arc::new(FcFontCache::default());
    let app_data = Arc::new(RefCell::new(RefAny::new(RecordingState::default())));
    let icon_provider = SharedIconProvider::from_handle(IconProviderHandle::default());

    let mut options = WindowCreateOptions::default();
    options.window_state.size.dimensions =
        azul_core::geom::LogicalSize { width: 1024.0, height: 768.0 };
    let cb: extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom = recording_layout;
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

#[test]
fn initial_layout_call_reports_reason_initial() {
    let mut window = make_window();

    // The pending reason on a fresh window must be Initial.
    assert_eq!(window.pending_relayout_reason(), RelayoutReason::Initial);

    window.regenerate_layout().expect("initial regenerate_layout");

    let obs = read_observations(&window);
    assert_eq!(obs.len(), 1, "layout() must run exactly once on the first regen");
    assert_eq!(obs[0].reason, RelayoutReason::Initial);
    assert_eq!(obs[0].width, 1024.0);
    assert_eq!(obs[0].height, 768.0);
}

#[test]
fn resize_re_invokes_layout_with_resize_reason_and_new_size() {
    let mut window = make_window();
    window.regenerate_layout().expect("initial regenerate_layout");

    // Cross the 768px breakpoint so a real platform path would also
    // request a regen. simulate_resize updates the window size and
    // queues the Resize reason.
    window.simulate_resize(320.0, 600.0);
    assert_eq!(window.pending_relayout_reason(), RelayoutReason::Resize);

    window.regenerate_layout().expect("resize regenerate_layout");

    let obs = read_observations(&window);
    assert_eq!(obs.len(), 2, "layout() must run again after a resize");

    let resize_call = &obs[1];
    assert_eq!(
        resize_call.reason,
        RelayoutReason::Resize,
        "the resize-triggered call must report RelayoutReason::Resize",
    );
    assert_eq!(resize_call.width, 320.0, "info.window_size reflects the new width");
    assert_eq!(resize_call.height, 600.0);
}

#[test]
fn untagged_relayout_after_resize_reports_refresh_dom() {
    // After a resize-tagged relayout consumes the reason, the next
    // untagged regen (e.g. RefAny mutation -> Update::RefreshDom) must
    // see RelayoutReason::RefreshDom — the implicit reason for an
    // app-state-driven re-render.
    let mut window = make_window();
    window.regenerate_layout().expect("initial");

    window.simulate_resize(320.0, 600.0);
    window.regenerate_layout().expect("resize");

    // No simulate_resize call this time: nothing tagged the upcoming
    // regen. The reason must reset to RefreshDom.
    assert_eq!(window.pending_relayout_reason(), RelayoutReason::RefreshDom);

    window.regenerate_layout().expect("untagged");

    let obs = read_observations(&window);
    assert_eq!(obs.len(), 3);
    assert_eq!(obs[2].reason, RelayoutReason::RefreshDom);
    assert_eq!(obs[2].width, 320.0, "size still reflects the previous resize");
}
