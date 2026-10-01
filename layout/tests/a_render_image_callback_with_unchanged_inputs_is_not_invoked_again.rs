//! A `RenderImageCallback` whose inputs did not change is not invoked again.
//!
//! PR #476 ledger, idle leftovers (FB3 2026-09-30): AzReview's per-sheet
//! `RenderImageCallback` images were invoked on EVERY frame
//! (`LayoutWindow::prepare_frame_content`); each invocation minted a new
//! `ImageRef`, the display list item compared unequal, the image rects were
//! repainted at rest and the window never went idle.
//!
//! A callback's inputs are its declared image (the DOM's callback `ImageRef`,
//! which a rebuild replaces), its box (logical size and hidpi factor) and an
//! explicit `update_image_callback` / `update_all_image_callbacks`. With the
//! same inputs a frame does not invoke it: no new `ImageRef`, no damage.
//!
//! Not compiled by the author (house rule).

use std::sync::atomic::{AtomicUsize, Ordering};

use azul_core::{
    dom::Dom,
    geom::LogicalSize,
    refany::RefAny,
    resources::{ImageRef, RawImageFormat, RendererResources},
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::{ExternalSystemCallbacks, RenderImageCallback, RenderImageCallbackInfo},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// Invocations of [`counting_canvas`] (only the one test below uses it).
static CALLS: AtomicUsize = AtomicUsize::new(0);

/// A canvas that renders a FRESH image every time it is asked, as an app
/// that does not cache its frames does.
extern "C" fn counting_canvas(_data: RefAny, _info: RenderImageCallbackInfo) -> ImageRef {
    CALLS.fetch_add(1, Ordering::SeqCst);
    ImageRef::null_image(4, 4, RawImageFormat::BGRA8, Vec::new())
}

fn lay_out(lw: &mut LayoutWindow, canvas: &ImageRef, width: f32) {
    let dom = Dom::create_body().with_child(
        Dom::create_image(canvas.clone()).with_css(&format!("width: {width}px; height: 50px;")),
    );
    let styled = StyledDom::create_from_dom(dom);
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = None;
    lw.layout_and_generate_display_list(styled, &ws, &rr, &sc, &mut dbg)
        .expect("the canvas lays out");
}

#[test]
fn a_render_image_callback_with_unchanged_inputs_is_not_invoked_again() {
    let canvas = ImageRef::callback(
        RenderImageCallback::create(counting_canvas).to_core(),
        RefAny::new(()),
    );
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lay_out(&mut lw, &canvas, 100.0);

    lw.prepare_frame_content();
    assert_eq!(CALLS.load(Ordering::SeqCst), 1, "the first frame renders the canvas");

    lw.prepare_frame_content();
    lw.prepare_frame_content();
    assert_eq!(
        CALLS.load(Ordering::SeqCst),
        1,
        "frames at rest must not re-invoke a canvas whose inputs did not change (each \
         invocation mints a new ImageRef and repaints its rect: the window never idles)"
    );

    // A new box is a new input: the canvas renders at its new size.
    lay_out(&mut lw, &canvas, 200.0);
    lw.prepare_frame_content();
    assert_eq!(CALLS.load(Ordering::SeqCst), 2, "a resized canvas renders again");

    // Laying out again at the same size changes nothing it reads.
    lay_out(&mut lw, &canvas, 200.0);
    lw.prepare_frame_content();
    assert_eq!(CALLS.load(Ordering::SeqCst), 2);
}

/// Invocations of [`counting_canvas_b`] (only the test below uses it).
static CALLS_B: AtomicUsize = AtomicUsize::new(0);

extern "C" fn counting_canvas_b(_data: RefAny, _info: RenderImageCallbackInfo) -> ImageRef {
    CALLS_B.fetch_add(1, Ordering::SeqCst);
    ImageRef::null_image(4, 4, RawImageFormat::BGRA8, Vec::new())
}

#[test]
fn an_explicit_update_renders_a_canvas_at_rest_again() {
    let canvas = ImageRef::callback(
        RenderImageCallback::create(counting_canvas_b).to_core(),
        RefAny::new(()),
    );
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lay_out(&mut lw, &canvas, 120.0);
    lw.prepare_frame_content();
    lw.prepare_frame_content();
    assert_eq!(CALLS_B.load(Ordering::SeqCst), 1);

    // `update_all_image_callbacks` (an animating GL texture's timer).
    lw.invalidate_all_image_callbacks();
    lw.prepare_frame_content();
    assert_eq!(CALLS_B.load(Ordering::SeqCst), 2);

    // `update_image_callback(dom, node)` for this one canvas.
    let (dom, node) = *lw
        .image_callback_inputs
        .keys()
        .next()
        .expect("the canvas's inputs are remembered");
    lw.invalidate_image_callback(dom, node);
    lw.prepare_frame_content();
    assert_eq!(CALLS_B.load(Ordering::SeqCst), 3);
    lw.prepare_frame_content();
    assert_eq!(CALLS_B.load(Ordering::SeqCst), 3, "and then it rests again");
}
