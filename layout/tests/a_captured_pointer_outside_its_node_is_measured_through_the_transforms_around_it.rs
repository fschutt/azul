//! A pointer that has LEFT its node while the node holds the capture (a drag past the edge of a pad,
//! a slider, a scrollbar) is measured against the node the way a pointer over the node is: through the
//! `transform` of its ancestors and of the node itself. `CallbackInfo::get_cursor_relative_to_node` for
//! such a pointer subtracted the untransformed layout origin from the window position, so under
//! `transform: scale(0.75)` a drag that leaves the pad sent a pan of (-0.19, +0.11) where the same
//! drag at 100 percent sent (-0.39, 0.00): the pointer moved `scale` times as far on the screen as in
//! the node.
//!
//! The page: body > frame > pad. The pad is a 100 x 100 box at (10, 20) of the frame (the frame's
//! padding). The pointer is at (-30, 250) in the pad's own box - left of it and far below it - and the
//! window position it has there is the frame's transform applied to the pad's layout position: the node
//! must read (-30, 250) at every scale.

use std::sync::Arc;

use azul_core::{
    callbacks::Update,
    dom::{Dom, DomId, DomNodeId, EventFilter, HoverEventFilter, NodeId},
    geom::LogicalPosition,
    gl::OptionGlContextPtr,
    refany::RefAny,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
    window::{CursorPosition, RawWindowHandle},
};
use azul_css::system::SystemStyle;
use azul_layout::{
    callbacks::{Callback, CallbackInfo, ExternalSystemCallbacks},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// body (0) > frame (1) > pad (2).
const PAD: NodeId = NodeId::new(2);

/// What the pad's callback was told.
#[derive(Default)]
struct Seen {
    cursor: Option<(f32, f32)>,
}

extern "C" fn on_move(mut data: RefAny, info: CallbackInfo) -> Update {
    if let Some(mut seen) = data.downcast_mut::<Seen>() {
        seen.cursor = info
            .get_cursor_relative_to_node()
            .into_option()
            .map(|p| (p.x, p.y));
    }
    Update::DoNothing
}

/// The page, its frame under `frame_css` and its pad under `pad_css` on top of the fixed geometry.
fn window(frame_css: &str, pad_css: &str) -> (LayoutWindow, RefAny) {
    let seen = RefAny::new(Seen::default());
    let pad = Dom::create_div()
        .with_css(&format!("width: 100px; height: 100px; {pad_css}"))
        .with_callback(
            EventFilter::Hover(HoverEventFilter::MouseMove),
            seen.clone(),
            Callback::from_ptr(on_move).to_core(),
        );
    let frame = Dom::create_div()
        .with_css(&format!(
            "width: 400px; height: 400px; padding: 20px 0 0 10px; transform-origin: 0 0; {frame_css}"
        ))
        .with_child(pad);
    let dom = Dom::create_body().with_css("margin: 0;").with_child(frame);

    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = azul_core::geom::LogicalSize::new(800.0, 600.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    (lw, seen)
}

/// What the pad's move callback reads as the pointer's place in the pad when the pointer is at
/// `cursor` of the window and over nothing (it left the pad).
fn read_at(lw: &mut LayoutWindow, seen: &RefAny, cursor: LogicalPosition) -> Option<(f32, f32)> {
    let sd = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
    let core = sd.node_data.as_container()[PAD]
        .get_callbacks()
        .as_ref()
        .iter()
        .find(|cb| cb.event == EventFilter::Hover(HoverEventFilter::MouseMove))
        .map(|cb| cb.callback.clone())
        .expect("harness: the pad listens to moves");
    let mut callback = Callback::from_core(core);
    let mut data = seen.clone();
    let mut state = lw.current_window_state.clone();
    state.mouse_state.cursor_position = CursorPosition::InWindow(cursor);
    let hit = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(PAD)),
    };
    let _ = lw.invoke_single_callback_at(
        hit,
        &mut callback,
        &mut data,
        &RawWindowHandle::Unsupported,
        &OptionGlContextPtr::None,
        Arc::new(SystemStyle::default()),
        &ExternalSystemCallbacks::rust_internal(),
        &None,
        &state,
        &RendererResources::default(),
    );
    data.downcast_ref::<Seen>().and_then(|s| s.cursor)
}

fn assert_reads(read: Option<(f32, f32)>, expected: (f32, f32), what: &str) {
    let (x, y) = read.unwrap_or_else(|| panic!("{what}: the callback was told nothing"));
    assert!(
        (x - expected.0).abs() < 0.05 && (y - expected.1).abs() < 0.05,
        "{what}: ({x}, {y}), not {expected:?}"
    );
}

/// The pad's place at (10, 20) of the window, the pointer at (-30, 250) of the pad.
const POINTER_IN_PAD: (f32, f32) = (-30.0, 250.0);

#[test]
fn without_a_transform_the_pointer_is_measured_from_the_pads_corner() {
    let (mut lw, seen) = window("", "");
    // premise: the corner of the pad is at (10, 20)
    assert_reads(
        read_at(&mut lw, &seen, LogicalPosition::new(10.0 - 30.0, 20.0 + 250.0)),
        POINTER_IN_PAD,
        "scale 1",
    );
}

#[test]
fn under_a_scaled_ancestor_the_pointer_reads_what_it_reads_at_scale_one() {
    for scale in [0.75_f32, 0.5, 2.0] {
        let (mut lw, seen) = window(&format!("transform: scale({scale});"), "");
        // the pad is at (10, 20) of the frame, the frame's corner at (0, 0): the window is the scale
        // times the layout position
        let window_pos =
            LogicalPosition::new((10.0 - 30.0) * scale, (20.0 + 250.0) * scale);
        assert_reads(
            read_at(&mut lw, &seen, window_pos),
            POINTER_IN_PAD,
            &format!("scale {scale}"),
        );
    }
}

#[test]
fn under_a_moved_and_scaled_ancestor_too() {
    let (mut lw, seen) = window("transform: translate(30px, 10px) scale(0.5);", "");
    let window_pos = LogicalPosition::new(30.0 + (10.0 - 30.0) * 0.5, 10.0 + (20.0 + 250.0) * 0.5);
    assert_reads(
        read_at(&mut lw, &seen, window_pos),
        POINTER_IN_PAD,
        "translate(30px, 10px) scale(0.5)",
    );
}

#[test]
fn under_the_pads_own_transform() {
    // the pad's own box is drawn at half its size about its corner (10, 20)
    let (mut lw, seen) = window("", "transform: scale(0.5); transform-origin: 0 0;");
    let window_pos = LogicalPosition::new(10.0 + (-30.0) * 0.5, 20.0 + 250.0 * 0.5);
    assert_reads(read_at(&mut lw, &seen, window_pos), POINTER_IN_PAD, "the pad at half size");
}
