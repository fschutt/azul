//! End-to-end proof that `CallbackInfo::override_node_css_properties`
//! lands in `CssPropertyCache::user_overridden_properties` after the
//! HeadlessWindow event loop flushes transactions.
//!
//! The unit path is obvious (write-through vec), but the public wiring
//! spans four crates:
//!
//!   1. `CallbackChange::OverrideNodeCssProperties` (azul-layout)
//!   2. `apply_user_change` match arm (azul-dll event loop)
//!   3. `StyledDom::restyle_user_property` writer (azul-core)
//!   4. `CssPropertyCache::user_overridden_properties` Vec (azul-core)
//!
//! This test exercises the whole chain: an `AfterMount` callback fires
//! during `regenerate_layout`, calls `override_node_css_properties`, and
//! the drained changes are applied before `regenerate_layout` returns.
//! We then inspect the cache directly.

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use azul_core::callbacks::{LayoutCallback, LayoutCallbackInfo, Update};
use azul_core::dom::{Dom, DomId, NodeData};
use azul_core::events::{ComponentEventFilter, EventFilter};
use azul_core::icon::{IconProviderHandle, SharedIconProvider};
use azul_core::id::NodeId;
use azul_core::refany::RefAny;
use azul_core::resources::AppConfig;
use azul_css::props::layout::dimensions::LayoutWidth;
use azul_css::props::property::{CssProperty, CssPropertyType};
use azul_layout::callbacks::{Callback, CallbackInfo};
use azul_layout::window_state::WindowCreateOptions;
use rust_fontconfig::FcFontCache;

use azul::desktop::shell2::headless::HeadlessWindow;

#[derive(Clone)]
struct TestState {
    /// Incremented once each time the AfterMount callback runs. Confirms the
    /// callback path executed at least once (sanity check against a silent
    /// regression in lifecycle dispatch).
    mount_fires: Arc<AtomicU32>,
    /// Frame counter — drives the `layout_cb` through a DOM transition so
    /// reconciliation produces a Mount event on frame 0→1.
    frame: Arc<AtomicU32>,
}

impl TestState {
    fn new() -> Self {
        Self {
            mount_fires: Arc::new(AtomicU32::new(0)),
            frame: Arc::new(AtomicU32::new(0)),
        }
    }
}

/// `AfterMount` callback that pushes an `override_node_css_properties`
/// change through the transaction system. The overridden property is
/// applied by the event loop after the callback returns.
extern "C" fn on_mount_override(mut data: RefAny, mut info: CallbackInfo) -> Update {
    if let Some(state) = data.downcast_ref::<TestState>() {
        state.mount_fires.fetch_add(1, Ordering::SeqCst);
    }

    // Override width on the root DOM's node 1 (the text child we added
    // below) with a value that is not the cascaded default. A real-world
    // usage would be an animation callback updating `opacity` or
    // `transform` every frame.
    let width_override =
        CssProperty::const_width(LayoutWidth::Px(azul_css::props::basic::PixelValue::px(123.0)));
    info.override_node_css_properties(
        DomId::ROOT_ID,
        NodeId::new(1),
        vec![width_override].into(),
    );

    Update::DoNothing
}

extern "C" fn layout_cb(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let state = match data.downcast_ref::<TestState>() {
        Some(s) => s.clone(),
        None => return Dom::create_body(),
    };

    // Frame 0 is empty → frame 1 mounts the child. Reconciliation against
    // the empty frame-0 DOM is what produces the AfterMount event.
    let frame = state.frame.fetch_add(1, Ordering::SeqCst);
    if frame == 0 {
        return Dom::create_body();
    }

    let mount_cb = Callback {
        cb: on_mount_override,
        ctx: azul_core::refany::OptionRefAny::None,
    }
    .to_core();

    let mut child = NodeData::create_text("target");
    child.add_callback(
        EventFilter::Component(ComponentEventFilter::AfterMount),
        RefAny::new(state),
        mount_cb,
    );

    Dom::create_body().with_child(Dom::create_from_data(child))
}

fn make_window(state: TestState) -> HeadlessWindow {
    let fc_cache = Arc::new(FcFontCache::default());
    let app_data = Arc::new(RefCell::new(RefAny::new(state)));
    let icon_provider = SharedIconProvider::from_handle(IconProviderHandle::default());

    let mut options = WindowCreateOptions::default();
    options.window_state.layout_callback = LayoutCallback {
        cb: layout_cb,
        ctx: azul_core::refany::OptionRefAny::None,
    };

    HeadlessWindow::new(
        options,
        app_data,
        AppConfig::default(),
        icon_provider,
        fc_cache,
        None,
    )
    .expect("HeadlessWindow construction must succeed")
}

#[test]
fn override_node_css_properties_lands_in_user_overridden_cache() {
    let state = TestState::new();
    let mut window = make_window(state.clone());

    // Frame 0 → build initial DOM. reconciliation vs. empty previous DOM
    // treats every node as newly-mounted, so AfterMount fires for the
    // text child, which pushes the OverrideNodeCssProperties change.
    // The event loop drains that transaction before regenerate_layout
    // returns.
    window
        .regenerate_layout()
        .expect("frame 0 regenerate_layout");
    // Frame 1 → no DOM change (layout_cb is deterministic), so the
    // AfterMount fire count should stay at exactly one. More importantly,
    // we re-run regenerate_layout so the override survives a second
    // restyle / layout pass.
    window
        .regenerate_layout()
        .expect("frame 1 regenerate_layout");

    let mount_fires = state.mount_fires.load(Ordering::SeqCst);
    assert!(
        mount_fires >= 1,
        "AfterMount callback should have fired at least once, got {}",
        mount_fires,
    );

    // Inspect the cache directly — confirms the writer actually ran and
    // that the override survived across the second regenerate_layout call.
    let layout_window = window
        .common
        .layout_window
        .as_ref()
        .expect("layout_window present after regenerate_layout");
    let layout_result = layout_window
        .layout_results
        .get(&DomId::ROOT_ID)
        .expect("root DOM layout result exists");

    let cache = layout_result.styled_dom.get_css_property_cache();
    let vec_for_node = cache
        .user_overridden_properties
        .get(1)
        .expect(
            "user_overridden_properties vec must cover node 1 after the \
             writer grew it to node_count",
        );

    let width_entry = vec_for_node
        .iter()
        .find(|(ty, _)| *ty == CssPropertyType::Width)
        .expect("override writer must have inserted the Width override");

    match &width_entry.1 {
        CssProperty::Width(lw) => match lw.get_property().expect("non-auto width override") {
            LayoutWidth::Px(px) => {
                let val = px.to_pixels_internal(0.0, 0.0, 0.0);
                assert!(
                    (val - 123.0).abs() < 0.5,
                    "override width value round-trip: expected 123.0, got {}",
                    val,
                );
            }
            other => panic!("override Width has wrong LayoutWidth variant: {:?}", other),
        },
        other => panic!("override entry for Width has wrong variant: {:?}", other),
    }
}
