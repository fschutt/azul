//! A4, the GPU path (WebRender integration).
//!
//! Counts, not times: what a transaction holds and what a translation emits.

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::LogicalSize,
    resources::{OpacityKey, RendererResources},
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;
use webrender::render_api::FrameMsg;

use crate::desktop::wr_translate2::{synchronize_gpu_values, WrTransaction};

/// A laid-out window of one box.
fn window() -> LayoutWindow {
    let dom = Dom::create_body()
        .with_child(Dom::create_div().with_css("width: 40px; height: 20px; background: #3366cc;"));
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(200.0, 100.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the window lays out");
    lw
}

/// `node` of the root DOM gets a bound opacity of `value` (what an animation
/// tick or a `set_css_property` of a GPU-only value writes).
fn bind_opacity(lw: &mut LayoutWindow, node: usize, key: OpacityKey, value: f32) {
    let cache = lw.gpu_state_manager.get_or_create_cache(DomId::ROOT_ID);
    cache.opacity_keys.insert(NodeId::new(node), key);
    cache.current_opacity_values.insert(NodeId::new(node), value);
}

/// How many values the frame operations of `txn` leave PENDING in WebRender,
/// replayed the way `SceneProperties` applies them: a reset empties the
/// pending set, an append adds to it.
fn pending_after(pending: &mut usize, txn: WrTransaction) -> (usize, usize) {
    let (mut resets, mut appends) = (0, 0);
    for op in txn.get_frame_ops() {
        match op {
            FrameMsg::ResetDynamicProperties => {
                resets += 1;
                *pending = 0;
            }
            FrameMsg::AppendDynamicProperties(properties) => {
                appends += 1;
                *pending +=
                    properties.transforms.len() + properties.floats.len() + properties.colors.len();
            }
            FrameMsg::AppendDynamicTransformProperties(transforms) => {
                appends += 1;
                *pending += transforms.len();
            }
            _ => {}
        }
    }
    (resets, appends)
}

/// A4-01. A changed set of GPU values REPLACES the set WebRender holds: the
/// pending values after n changed frames are the window's bound values, not n
/// times as many. Mutation: remove `txn.reset_dynamic_properties()` from
/// `synchronize_gpu_values` and the count after frame 16 is 16 times the count
/// after frame 1.
#[test]
fn a4_01_changed_gpu_values_replace_the_set_webrender_holds() {
    let mut lw = window();
    let key = OpacityKey::unique();
    let mut pending = 0usize;
    let mut after_first = 0usize;
    for frame in 0..16u32 {
        bind_opacity(&mut lw, 0, key, 0.25 + frame as f32 / 64.0);
        let mut txn = WrTransaction::new();
        assert!(
            synchronize_gpu_values(&mut lw, &mut txn),
            "frame {frame}: the value moved, so the set is sent"
        );
        let (resets, appends) = pending_after(&mut pending, txn);
        assert_eq!((resets, appends), (1, 1), "frame {frame}: one reset, then one append");
        if frame == 0 {
            after_first = pending;
        }
    }
    assert!(after_first >= 1, "the bound opacity is in the set");
    assert_eq!(
        pending, after_first,
        "16 changed frames leave the set once, not 16 times ({after_first} values a frame)"
    );

    // An unchanged set is not sent at all.
    let mut txn = WrTransaction::new();
    assert!(!synchronize_gpu_values(&mut lw, &mut txn));
    assert!(txn.is_empty(), "nothing in the transaction for values that stood still");
}

/// A4-07. The digest that says "the same bound values as last time" is a sum of
/// one hash per (key, value): the order the values come in (hash maps) does not
/// change it, two keys that swap their values do, and a NaN is equal to itself
/// (bitwise), so an unchanged set is never sent and a changed one always.
/// Mutation: hashing the value without its key in `synchronize_gpu_values`
/// makes the swap "unchanged".
#[test]
fn a4_07_the_digest_of_the_bound_values_sees_a_swap_and_not_the_order() {
    let mut lw = window();
    let (a, b) = (OpacityKey::unique(), OpacityKey::unique());
    let mut sync = |lw: &mut LayoutWindow| synchronize_gpu_values(lw, &mut WrTransaction::new());

    bind_opacity(&mut lw, 0, a, 0.25);
    bind_opacity(&mut lw, 1, b, 0.75);
    assert!(sync(&mut lw), "the first set is sent");
    assert!(!sync(&mut lw), "the same set is not");

    bind_opacity(&mut lw, 0, a, 0.75);
    bind_opacity(&mut lw, 1, b, 0.25);
    assert!(sync(&mut lw), "the two keys swapped their values: a change");
    assert!(!sync(&mut lw));

    bind_opacity(&mut lw, 0, a, f32::NAN);
    assert!(sync(&mut lw), "a NaN is a new value");
    assert!(!sync(&mut lw), "and equal to itself the next time");

    // A full transaction forgets what was sent: the same set goes again.
    lw.gpu_state_manager.invalidate_submitted_digests();
    assert!(sync(&mut lw));
}

/// `node` of the root DOM gets a bound CSS transform (what `set_css_property`
/// of `transform` writes for a node the display list already binds).
fn bind_transform(
    lw: &mut LayoutWindow,
    node: usize,
    key: azul_core::resources::TransformKey,
    transform: azul_core::transform::ComputedTransform3D,
) {
    let cache = lw.gpu_state_manager.get_or_create_cache(DomId::ROOT_ID);
    cache.css_transform_keys.insert(NodeId::new(node), key);
    cache
        .css_current_transform_values
        .insert(NodeId::new(node), transform);
}

/// `translate(tx, 0) scale(k)`.
fn scaled(k: f32, tx: f32) -> azul_core::transform::ComputedTransform3D {
    let mut m = azul_core::transform::ComputedTransform3D::IDENTITY;
    m.m[0][0] = k;
    m.m[1][1] = k;
    m.m[3][0] = tx;
    m
}

/// The "is being zoomed" marks a transaction carries: (on, key).
fn zoom_marks(txn: WrTransaction) -> Vec<(bool, u64)> {
    txn.get_frame_ops()
        .into_iter()
        .filter_map(|op| match op {
            FrameMsg::SetIsTransformAsyncZooming(on, id) => Some((on, id.to_u64())),
            _ => None,
        })
        .collect()
}

/// A4-02. A transform whose scale moves from one set to the next is marked as
/// being zoomed, once (WebRender then rasterises the text under it once, in
/// local space, instead of every glyph again at every step); a translation
/// alone marks nothing; a value back at no scale is drawn sharp at once; one
/// that stands still is drawn sharp when the settle time is over, in a frame of
/// its own, and until then the window owes frames.
/// Mutations: marking on every changed set (not only when the linear part
/// moved) fails "a move alone"; not returning `true` for the settle frame fails
/// "the sharp frame is a frame".
#[test]
fn a4_02_a_moving_scale_is_marked_as_zoomed_once_and_drawn_sharp_at_rest() {
    use std::time::Duration;

    use crate::desktop::wr_translate2::{forget_live_transforms, TEST_ZOOM_SETTLE};

    TEST_ZOOM_SETTLE.with(|settle| settle.set(Some(Duration::from_secs(3600))));
    let mut lw = window();
    forget_live_transforms(&lw.document_id);
    let key = azul_core::resources::TransformKey::unique();
    let id = key.id as u64;
    let step = |lw: &mut LayoutWindow, k: f32, tx: f32| {
        bind_transform(lw, 0, key, scaled(k, tx));
        let mut txn = WrTransaction::new();
        let changed = synchronize_gpu_values(lw, &mut txn);
        (changed, zoom_marks(txn))
    };

    assert_eq!(step(&mut lw, 1.0, 0.0), (true, vec![]), "the first set: nothing has moved");
    assert_eq!(step(&mut lw, 1.5, 0.0), (true, vec![(true, id)]), "the scale moved");
    assert!(lw.gpu_state_manager.frames_owed(), "it has to be drawn sharp later");
    assert_eq!(step(&mut lw, 1.6, 4.0), (true, vec![]), "still moving: marked once");
    assert_eq!(step(&mut lw, 1.6, 9.0), (true, vec![]), "a move alone marks nothing");
    assert_eq!(
        step(&mut lw, 1.0, 0.0),
        (true, vec![(false, id)]),
        "back at no scale: sharp at once"
    );
    assert!(!lw.gpu_state_manager.frames_owed());

    // A scale that stands still: sharp when the settle time is over.
    assert_eq!(step(&mut lw, 2.0, 0.0).1, vec![(true, id)]);
    assert!(lw.gpu_state_manager.frames_owed());
    TEST_ZOOM_SETTLE.with(|settle| settle.set(Some(Duration::ZERO)));
    let mut txn = WrTransaction::new();
    assert!(
        synchronize_gpu_values(&mut lw, &mut txn),
        "the sharp frame is a frame, though no value changed"
    );
    assert_eq!(zoom_marks(txn), vec![(false, id)]);
    assert!(!lw.gpu_state_manager.frames_owed(), "nothing settles any more");
    assert!(
        !synchronize_gpu_values(&mut lw, &mut WrTransaction::new()),
        "and the window is idle"
    );
    TEST_ZOOM_SETTLE.with(|settle| settle.set(None));
    forget_live_transforms(&lw.document_id);
}
