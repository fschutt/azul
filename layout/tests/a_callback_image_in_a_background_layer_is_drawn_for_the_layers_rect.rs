//! A callback image used as a BACKGROUND layer is rendered for the layer's
//! rect, and its frame is what the display list draws.
//!
//! `background: url(id)` (and `builtin(<name>)`, which the built-in 2.5D
//! materials register as callback images) naming an image the window's cache
//! holds as a `RenderImageCallback` put the DECLARED callback image into the
//! display list, and nothing ever invoked it: frame preparation
//! (`LayoutWindow::invoke_image_callbacks_into_overlay`) walks only
//! `NodeType::Image` nodes. The CPU raster painted the grey "never invoked"
//! placeholder over the whole box, WebRender drew nothing.
//!
//! Not compiled by the author (house rule).

use std::sync::Mutex;

use azul_core::{
    dom::{Dom, DomId},
    geom::{LogicalRect, LogicalSize},
    refany::RefAny,
    resources::{ImageRef, RawImage, RawImageData, RawImageFormat, RendererResources},
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::{ExternalSystemCallbacks, RenderImageCallback, RenderImageCallbackInfo},
    solver3::display_list::DisplayListItem,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// The logical size of every rect [`painted_canvas`] was asked to fill.
static ASKED: Mutex<Vec<(f32, f32)>> = Mutex::new(Vec::new());

/// A canvas that paints its rect opaque white, at the rect's device size.
extern "C" fn painted_canvas(_data: RefAny, info: RenderImageCallbackInfo) -> ImageRef {
    let bounds = info.get_bounds();
    let logical = bounds.get_logical_size();
    ASKED
        .lock()
        .expect("not poisoned")
        .push((logical.width, logical.height));
    let physical = bounds.get_physical_size();
    let (w, h) = (
        physical.width.max(1) as usize,
        physical.height.max(1) as usize,
    );
    ImageRef::new_rawimage(RawImage {
        pixels: RawImageData::U8(vec![255_u8; w * h * 4].into()),
        width: w,
        height: h,
        premultiplied_alpha: true,
        data_format: RawImageFormat::RGBA8,
        tag: Vec::new().into(),
    })
    .expect("an RGBA8 frame")
}

/// A 120 x 40 div whose background is the canvas registered as
/// `painted-canvas`.
fn lay_out(lw: &mut LayoutWindow) {
    let dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_css("width: 120px; height: 40px; background: url(painted-canvas);"),
    );
    let styled = StyledDom::create_from_dom(dom);
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = None;
    lw.layout_and_generate_display_list(styled, &ws, &rr, &sc, &mut dbg)
        .expect("the div lays out");
}

/// Every image item of the root display list: its rect and its image.
fn image_items(lw: &LayoutWindow) -> Vec<(LogicalRect, ImageRef)> {
    let lr = lw.get_layout_result(&DomId::ROOT_ID).expect("laid out");
    lr.display_list
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Image { bounds, image, .. } => Some((*bounds.inner(), image.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn a_callback_image_in_a_background_layer_is_drawn_for_the_layers_rect() {
    let canvas = ImageRef::callback(
        RenderImageCallback::create(painted_canvas).to_core(),
        RefAny::new(()),
    );
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lw.image_cache
        .add_css_image_id("painted-canvas".into(), canvas.clone());
    lay_out(&mut lw);
    lw.prepare_frame_content();

    let items = image_items(&lw);
    assert_eq!(items.len(), 1, "the div's one background layer: {items:?}");
    let (rect, image) = &items[0];
    assert_eq!((rect.size.width, rect.size.height), (120.0, 40.0));
    assert!(
        !image.is_callback(),
        "the display list still draws the declared callback image, which no renderer can draw \
         (the CPU raster paints the grey placeholder over the box)"
    );
    assert_ne!(image.get_hash(), canvas.get_hash());
    assert_eq!(
        image.get_size(),
        LogicalSize::new(120.0, 40.0),
        "a frame of the layer's own size"
    );
    assert_eq!(
        ASKED.lock().expect("not poisoned").as_slice(),
        &[(120.0, 40.0)],
        "rendered once, for the layer's rect"
    );

    // A frame at rest renders nothing again and keeps its frame.
    lw.prepare_frame_content();
    lw.prepare_frame_content();
    assert_eq!(ASKED.lock().expect("not poisoned").len(), 1);
    let again = image_items(&lw);
    assert_eq!(again.len(), 1);
    assert_eq!(again[0].1.get_hash(), image.get_hash(), "the same frame");
}
