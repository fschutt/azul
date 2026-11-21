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

/// A10-15 (the GPU document's size, applied by A4). The rule the shells now use
/// when they create the document is the one of every frame's document view,
/// viewport and framebuffer: `LogicalSize::to_physical`, rounded. 333.7 units
/// at 1.5 are 500.55 device px: 501 (truncated, as the macOS shell sized the
/// new document, 500: one pixel less than its own viewport). This pins the
/// rule, not the shells' call sites (they need a window).
#[test]
fn a10_15_the_gpu_document_is_sized_by_the_rule_of_the_viewport() {
    let dimensions = LogicalSize::new(333.7, 200.0);
    let created = dimensions.to_physical(1.5);
    assert_eq!((created.width, created.height), (501, 300));

    let mut ws = FullWindowState::default();
    ws.size.dimensions = dimensions;
    ws.size.dpi = 144;
    let every_frame = ws.size.get_physical_size();
    assert_eq!(
        (created.width, created.height),
        (every_frame.width, every_frame.height),
        "the document is born at the size its first transaction gives it"
    );
}
