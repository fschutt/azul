//! An app that repaints part of an image node says WHICH part changed, and
//! the renderer uploads only that part.
//!
//! A paint program shows its canvas through one image node and replaces the
//! node's image after every brush dab. Without a dirty rect, the GPU backends
//! re-uploaded the whole canvas per dab (a 2000x1500 canvas is 12 MB per pointer
//! move). `CallbackInfo::change_node_image_rect` carries the changed rect into
//! the content chokepoint, which keeps, per image node, the region the renderer
//! has not uploaded yet: the union of every partial change since the last
//! upload, or the whole image when a change did not say (or changed the size).

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::LogicalSize,
    resources::{ImageDirtyRect, ImageRef, RawImage, RawImageData, RawImageFormat, RendererResources},
    styled_dom::StyledDom,
};
use azul_css::props::basic::{LayoutPoint, LayoutRect, LayoutSize};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    overlay::{ContentChange, ContentDirtyTier},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const NODE: NodeId = NodeId::new(1);

fn canvas(w: usize, h: usize, fill: u8) -> ImageRef {
    ImageRef::new_rawimage(RawImage {
        pixels: RawImageData::U8(vec![fill; w * h * 4].into()),
        width: w,
        height: h,
        premultiplied_alpha: true,
        data_format: RawImageFormat::BGRA8,
        tag: Vec::new().into(),
    })
    .expect("a well-formed canvas")
}

fn rect(x: isize, y: isize, w: isize, h: isize) -> LayoutRect {
    LayoutRect::new(LayoutPoint::new(x, y), LayoutSize::new(w, h))
}

fn window_with_canvas() -> LayoutWindow {
    let dom = Dom::create_body()
        .with_css("margin: 0;")
        .with_children(
            vec![Dom::create_image(canvas(64, 64, 0)).with_css("width: 64px; height: 64px;")]
                .into(),
        );
    let styled = StyledDom::create_from_dom(dom);
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(120.0, 80.0);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled, &ws, &rr, &sc, &mut dbg)
        .unwrap();
    lw
}

fn change(lw: &mut LayoutWindow, image: &ImageRef, dirty: Option<LayoutRect>) -> ContentDirtyTier {
    lw.apply_content_change(ContentChange::Image {
        dom_id: DomId::ROOT_ID,
        node_id: NODE,
        image: image.clone(),
        dirty_rect: dirty,
    })
    .tier
}

fn pending(lw: &LayoutWindow) -> Option<ImageDirtyRect> {
    lw.content_overlay.image_dirty(DomId::ROOT_ID, NODE)
}

#[test]
fn a_partial_change_leaves_only_its_rect_for_the_renderer() {
    let mut lw = window_with_canvas();
    let tier = change(&mut lw, &canvas(64, 64, 1), Some(rect(8, 8, 4, 4)));
    assert_eq!(tier, ContentDirtyTier::Paint, "a same-size swap repaints");
    assert_eq!(pending(&lw), Some(ImageDirtyRect::Partial(rect(8, 8, 4, 4))));
}

#[test]
fn partial_changes_between_two_uploads_add_up() {
    let mut lw = window_with_canvas();
    change(&mut lw, &canvas(64, 64, 1), Some(rect(0, 0, 2, 2)));
    change(&mut lw, &canvas(64, 64, 2), Some(rect(10, 10, 2, 2)));
    assert_eq!(
        pending(&lw),
        Some(ImageDirtyRect::Partial(rect(0, 0, 12, 12))),
        "the renderer never saw the first image: the next upload covers both rects"
    );
}

#[test]
fn a_change_without_a_rect_is_uploaded_whole() {
    let mut lw = window_with_canvas();
    change(&mut lw, &canvas(64, 64, 1), None);
    assert_eq!(pending(&lw), Some(ImageDirtyRect::All));
    change(&mut lw, &canvas(64, 64, 2), Some(rect(1, 1, 1, 1)));
    assert_eq!(
        pending(&lw),
        Some(ImageDirtyRect::All),
        "a partial change after a whole one is still whole until the renderer took it"
    );
    lw.content_overlay.clear_image_dirty();
    change(&mut lw, &canvas(64, 64, 3), Some(rect(1, 1, 1, 1)));
    assert_eq!(pending(&lw), Some(ImageDirtyRect::Partial(rect(1, 1, 1, 1))));
}

#[test]
fn a_resized_image_is_uploaded_whole_whatever_rect_it_names() {
    let mut lw = window_with_canvas();
    let tier = change(&mut lw, &canvas(32, 32, 1), Some(rect(0, 0, 4, 4)));
    assert_eq!(tier, ContentDirtyTier::Relayout, "a new intrinsic size relayouts");
    assert_eq!(pending(&lw), Some(ImageDirtyRect::All));
}

#[test]
fn setting_the_same_image_again_changes_nothing_pending() {
    let mut lw = window_with_canvas();
    let image = canvas(64, 64, 1);
    change(&mut lw, &image, Some(rect(2, 2, 2, 2)));
    lw.content_overlay.clear_image_dirty();
    let tier = change(&mut lw, &image, Some(rect(30, 30, 9, 9)));
    assert_eq!(tier, ContentDirtyTier::Unchanged);
    assert_eq!(pending(&lw), None, "nothing new reached the node");
}
