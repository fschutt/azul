//! End-to-end proof that lifecycle events (`Mount` / `Unmount` / `Update` /
//! `Resize`) reach user callbacks through the full event-loop pipeline.
//!
//! The unit tests in `core/tests/reconciliation/deep_reconciliation.rs` only
//! verify that `reconcile_dom` *produces* the right `SyntheticEvent`s. They
//! cannot verify that those events make it through:
//!
//!   1. `regenerate_layout` queueing into `LayoutWindow.pending_lifecycle_events`,
//!   2. `dispatch_pending_lifecycle_events` draining the queue,
//!   3. `dispatch_events_propagated` matching by `EventFilter::Component(...)`,
//!   4. The user callback actually being invoked.
//!
//! This integration test exercises that whole chain by driving a real
//! `HeadlessWindow` whose layout callback returns different DOMs on
//! successive frames. A shared `Arc<AtomicU32>` smuggled through `RefAny`
//! lets us count callback invocations.

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use azul_core::callbacks::{LayoutCallback, LayoutCallbackInfo, Update};
use azul_core::dom::{Dom, NodeData};
use azul_core::events::{ComponentEventFilter, EventFilter};
use azul_core::icon::{IconProviderHandle, SharedIconProvider};
use azul_core::refany::RefAny;
use azul_core::resources::AppConfig;
use azul_layout::callbacks::{Callback, CallbackInfo};
use azul_layout::window_state::WindowCreateOptions;
use rust_fontconfig::FcFontCache;

use azul::desktop::shell2::headless::HeadlessWindow;

#[derive(Clone)]
struct Counters {
    mounts: Arc<AtomicU32>,
    unmounts: Arc<AtomicU32>,
    updates: Arc<AtomicU32>,
    /// Toggle the layout callback returns to force reconciliation deltas
    /// across successive `regenerate_layout` calls.
    frame: Arc<AtomicU32>,
}

impl Counters {
    fn new() -> Self {
        Self {
            mounts: Arc::new(AtomicU32::new(0)),
            unmounts: Arc::new(AtomicU32::new(0)),
            updates: Arc::new(AtomicU32::new(0)),
            frame: Arc::new(AtomicU32::new(0)),
        }
    }
}

extern "C" fn on_mount(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(c) = data.downcast_ref::<Counters>() {
        c.mounts.fetch_add(1, Ordering::SeqCst);
    }
    Update::DoNothing
}

extern "C" fn on_unmount(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(c) = data.downcast_ref::<Counters>() {
        c.unmounts.fetch_add(1, Ordering::SeqCst);
    }
    Update::DoNothing
}

extern "C" fn on_update(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(c) = data.downcast_ref::<Counters>() {
        c.updates.fetch_add(1, Ordering::SeqCst);
    }
    Update::DoNothing
}

extern "C" fn layout_cb(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let counters = match data.downcast_ref::<Counters>() {
        Some(c) => c.clone(),
        None => return Dom::create_body(),
    };
    let frame = counters.frame.fetch_add(1, Ordering::SeqCst);

    let mount_cb = Callback { cb: on_mount, ctx: azul_core::refany::OptionRefAny::None }.to_core();
    let unmount_cb =
        Callback { cb: on_unmount, ctx: azul_core::refany::OptionRefAny::None }.to_core();
    let update_cb =
        Callback { cb: on_update, ctx: azul_core::refany::OptionRefAny::None }.to_core();

    // Frame 0: empty body (no children).
    // Frame 1: body with two children — child A wired to AfterMount, child B
    //          wired to BeforeUnmount. This frame mounts BOTH callbacks.
    // Frame 2: child A is gone, child B remains, plus a new keyed child C
    //          with a different text content vs. nothing-on-frame-1 (so child
    //          C mounts). On the path from frame 1 → frame 2 we expect:
    //              - child A unmount (no callback though — A had AfterMount only)
    //              - child B unmount (B had BeforeUnmount → fires)
    //              - child C mount   (C has AfterMount → fires)
    // Frame 3: child C's text content changes; C's keyed identity stays. The
    //          Updated callback on C should fire.
    match frame {
        0 => Dom::create_body(),
        1 => {
            let mut a = NodeData::create_text("A");
            a.add_callback(
                EventFilter::Component(ComponentEventFilter::AfterMount),
                RefAny::new(counters.clone()),
                mount_cb,
            );
            let mut b = NodeData::create_text("B");
            b.add_callback(
                EventFilter::Component(ComponentEventFilter::BeforeUnmount),
                RefAny::new(counters.clone()),
                unmount_cb,
            );
            Dom::create_body()
                .with_child(Dom::create_from_data(a))
                .with_child(Dom::create_from_data(b))
        }
        2 => {
            // Keep B (so it'll see BeforeUnmount) — no, we *remove* both A and B
            // to force two unmounts; then add C with AfterMount + Updated so the
            // next frame can fire Updated on the same keyed node.
            let mut c = NodeData::create_text("v1").with_key(0xC0FFEEu64);
            c.add_callback(
                EventFilter::Component(ComponentEventFilter::AfterMount),
                RefAny::new(counters.clone()),
                mount_cb,
            );
            c.add_callback(
                EventFilter::Component(ComponentEventFilter::Updated),
                RefAny::new(counters.clone()),
                update_cb,
            );
            Dom::create_body().with_child(Dom::create_from_data(c))
        }
        _ => {
            // Same keyed C, but with new content — this is the Updated path.
            let mut c = NodeData::create_text("v2").with_key(0xC0FFEEu64);
            c.add_callback(
                EventFilter::Component(ComponentEventFilter::AfterMount),
                RefAny::new(counters.clone()),
                mount_cb,
            );
            c.add_callback(
                EventFilter::Component(ComponentEventFilter::Updated),
                RefAny::new(counters.clone()),
                update_cb,
            );
            Dom::create_body().with_child(Dom::create_from_data(c))
        }
    }
}

fn make_window(counters: Counters) -> HeadlessWindow {
    let fc_cache = Arc::new(FcFontCache::default());
    let app_data = Arc::new(RefCell::new(RefAny::new(counters)));
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
fn lifecycle_callbacks_fire_through_headless_event_loop() {
    let counters = Counters::new();
    let mut window = make_window(counters.clone());

    // Frame 0 → empty body. First regenerate_layout has no previous DOM, so
    // reconciliation is a no-op and no Mount/Unmount events fire on frame 0
    // itself. We still need the call to populate `layout_results`.
    window
        .regenerate_layout()
        .expect("frame 0 regenerate_layout");

    // Frame 1 → body with A (AfterMount) + B (BeforeUnmount). Two newly-
    // appeared nodes; A has a Mount callback and should fire it.
    window
        .regenerate_layout()
        .expect("frame 1 regenerate_layout");
    assert_eq!(
        counters.mounts.load(Ordering::SeqCst),
        1,
        "frame 0→1: child A's AfterMount callback must fire exactly once \
         (mount={}, unmount={}, update={})",
        counters.mounts.load(Ordering::SeqCst),
        counters.unmounts.load(Ordering::SeqCst),
        counters.updates.load(Ordering::SeqCst),
    );
    assert_eq!(
        counters.unmounts.load(Ordering::SeqCst),
        0,
        "frame 0→1: nothing was removed yet, BeforeUnmount must not fire"
    );

    // Frame 2 → A and B both removed, C added with AfterMount + Updated.
    //  - B has a BeforeUnmount callback → +1 unmount
    //  - C is brand new with AfterMount   → +1 mount
    //  - A had AfterMount only (no BeforeUnmount) → no unmount event fires
    window
        .regenerate_layout()
        .expect("frame 2 regenerate_layout");
    assert_eq!(
        counters.mounts.load(Ordering::SeqCst),
        2,
        "frame 1→2: C's AfterMount must fire (running mount total = 2). \
         (mount={}, unmount={}, update={})",
        counters.mounts.load(Ordering::SeqCst),
        counters.unmounts.load(Ordering::SeqCst),
        counters.updates.load(Ordering::SeqCst),
    );
    assert_eq!(
        counters.unmounts.load(Ordering::SeqCst),
        1,
        "frame 1→2: B's BeforeUnmount must fire exactly once"
    );

    // Frame 3 → same keyed C, new text content. Tier 1 (rec-key) match →
    // Updated fires.
    window
        .regenerate_layout()
        .expect("frame 3 regenerate_layout");
    assert_eq!(
        counters.updates.load(Ordering::SeqCst),
        1,
        "frame 2→3: keyed text change on C must fire Updated exactly once. \
         (mount={}, unmount={}, update={})",
        counters.mounts.load(Ordering::SeqCst),
        counters.unmounts.load(Ordering::SeqCst),
        counters.updates.load(Ordering::SeqCst),
    );
    // No new Mount or Unmount on this frame.
    assert_eq!(counters.mounts.load(Ordering::SeqCst), 2);
    assert_eq!(counters.unmounts.load(Ordering::SeqCst), 1);
}
