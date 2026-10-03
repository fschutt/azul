//! An image patched into the display list IN PLACE (a canvas's new frame, an
//! app's `change_node_image`) is still the image after the next relayout
//! that the solver serves from its structural-identity cache.
//!
//! Found by MEDIA6 (2026-10-03) with AzPaint: a stroke's frame was rendered
//! (`render_canvas ... RASTER`), patched into the list, and then the window
//! showed the canvas WITHOUT the stroke until the next resize. A paint-tier
//! image change does `Arc::make_mut(&mut display_list).patch_node_image(..)`;
//! the solver's `cached_display_list` holds a clone of the same Arc, so
//! `make_mut` copies and the cache keeps the PRE-PATCH list. The next
//! relayout over the same tree (a hover change, a `RefreshDom` that changed
//! no structure) is a cache hit and serves that pre-patch list back - and as
//! the canvas's inputs did not change, nothing renders it again. The CSS
//! transition patch path swaps the patched Arc into the cache for exactly
//! this reason; the image chokepoint did not.
//!
//! Not compiled by the author (house rule).

use std::sync::{Arc, Mutex};

use azul_core::{
    dom::{Dom, DomId, NodeId, NodeType},
    geom::LogicalSize,
    refany::RefAny,
    resources::{ImageRef, RawImage, RawImageFormat, RendererResources},
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::{ExternalSystemCallbacks, RenderImageCallback, RenderImageCallbackInfo},
    overlay::ContentChange,
    solver3::display_list::DisplayListItem,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// The ids of the frames [`frame_canvas`] produced, in order.
static FRAMES: Mutex<Vec<u64>> = Mutex::new(Vec::new());

/// A canvas that draws a NEW frame every time it is asked.
extern "C" fn frame_canvas(_data: RefAny, _info: RenderImageCallbackInfo) -> ImageRef {
    let frame = ImageRef::null_image(4, 4, RawImageFormat::BGRA8, Vec::new());
    FRAMES.lock().expect("frames").push(frame.get_hash().inner);
    frame
}

/// `body > img(image)` in a 400 x 300 window.
fn lay_out(lw: &mut LayoutWindow, image: &ImageRef) {
    let dom = Dom::create_body()
        .with_child(Dom::create_image(image.clone()).with_css("width: 100px; height: 50px;"));
    let styled = StyledDom::create_from_dom(dom);
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = None;
    lw.layout_and_generate_display_list(styled, &ws, &rr, &sc, &mut dbg)
        .expect("the image lays out");
}

/// The id of the image the display list paints (its one Image item).
fn painted(lw: &LayoutWindow) -> Option<u64> {
    let dl = &lw.get_layout_result(&DomId::ROOT_ID)?.display_list;
    dl.items.iter().find_map(|item| match item {
        DisplayListItem::Image { image, .. } => Some(image.get_hash().inner),
        _ => None,
    })
}

/// The image node's id.
fn image_node(lw: &LayoutWindow) -> NodeId {
    let sd = &lw.get_layout_result(&DomId::ROOT_ID).expect("laid out").styled_dom;
    let index = sd
        .node_data
        .as_ref()
        .iter()
        .position(|nd| matches!(nd.get_node_type(), NodeType::Image(_)))
        .expect("an image node");
    NodeId::new(index)
}

/// The list the solver's structural-identity cache serves next.
fn cached_list(lw: &LayoutWindow) -> Option<Arc<azul_layout::solver3::display_list::DisplayList>> {
    lw.layout_cache.cached_display_list.as_ref().map(|c| c.5.clone())
}

/// Lay the same tree out again and check the premise: the solver served it
/// from its cache (a fresh build would replace the cached list).
fn relayout_from_cache(lw: &mut LayoutWindow, image: &ImageRef) {
    let before = cached_list(lw).expect("a cached list after a layout");
    lay_out(lw, image);
    let after = cached_list(lw).expect("a cached list after a layout");
    assert!(
        Arc::ptr_eq(&before, &after),
        "premise: laying the identical tree out again is a structural cache hit"
    );
}

#[test]
fn a_canvas_frame_patched_in_place_survives_a_cached_relayout() {
    let canvas = ImageRef::callback(
        RenderImageCallback::create(frame_canvas).to_core(),
        RefAny::new(()),
    );
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lay_out(&mut lw, &canvas);
    lw.prepare_frame_content();

    // The app asks for a new frame (`update_image_callback`, a stroke).
    let (dom, node) = *lw
        .image_callback_inputs
        .keys()
        .next()
        .expect("the canvas's inputs are remembered");
    lw.invalidate_image_callback(dom, node);
    lw.prepare_frame_content();
    let newest = *FRAMES.lock().expect("frames").last().expect("frames were drawn");
    assert_eq!(FRAMES.lock().expect("frames").len(), 2, "premise: two frames");
    assert_eq!(painted(&lw), Some(newest), "premise: the new frame is patched in");

    // A hover change, a RefreshDom over the same structure: a cache hit.
    relayout_from_cache(&mut lw, &canvas);
    lw.prepare_frame_content();
    assert_eq!(FRAMES.lock().expect("frames").len(), 2, "nothing asked for a new frame");
    assert_eq!(
        painted(&lw),
        Some(newest),
        "the relayout served the PRE-PATCH list from the solver's cache: the canvas shows an \
         old frame (AzPaint: the stroke vanished until a resize)"
    );
}

fn solid(r: u8) -> ImageRef {
    let image = RawImage::create_rgba8(4, 4, [r, 0, 0, 255].repeat(16).into(), false);
    ImageRef::new_rawimage(image).expect("a 4 x 4 image")
}

#[test]
fn an_image_swapped_in_place_survives_a_cached_relayout() {
    let declared = solid(10);
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lay_out(&mut lw, &declared);
    assert_eq!(painted(&lw), Some(declared.get_hash().inner));

    // `change_node_image` with a picture of the same size: paint tier, the
    // list is patched in place (a video frame, AzVideoCut's monitor).
    let swapped = solid(200);
    let node = image_node(&lw);
    let _ = lw.apply_content_change(ContentChange::Image {
        dom_id: DomId::ROOT_ID,
        node_id: node,
        image: swapped.clone(),
        dirty_rect: None,
    });
    assert_eq!(painted(&lw), Some(swapped.get_hash().inner), "premise: patched in");

    relayout_from_cache(&mut lw, &declared);
    assert_eq!(
        painted(&lw),
        Some(swapped.get_hash().inner),
        "the relayout served the PRE-PATCH list: the node shows the image the app replaced"
    );
}
