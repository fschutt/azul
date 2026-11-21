//! An `<img src>` from markup shows the image the app put into the window's
//! image cache under that src.
//!
//! The XML loaders make `<img src="https://..">` an Image node whose image is a
//! placeholder carrying the src (`ImageRef::null_image` with the src's bytes as
//! its tag, `core/src/xml.rs`): the box had no size and nothing drew it, and an
//! app had no way to give it the picture. A mail client fetches a mail's
//! pictures when the reader asks ("download pictures") and registers each one
//! in the image cache under its address (`CallbackInfo::add_image_to_cache`,
//! the same ids `background-image: url(..)` resolves against): the `<img>` then
//! lays out at the picture's size and paints it, like a browser's.
//!
//! A src nobody supplied stays in the display list as the placeholder carrying
//! its src - a renderer that fetches pictures itself (printpdf's HTML bridge
//! lays out with an empty image cache and resolves `NullImage { tag: src }`)
//! reads it there - and the window's renderers draw nothing for it: WebRender
//! has no image to upload for a `NullImage`, and the CPU renderer must agree
//! (it painted the grey "no GPU / not produced yet" placeholder over a
//! 300x150 box).
//!
//! Not compiled by the author (house rule); expected RED before the fix.

use azul_core::{
    dom::{DomId, NodeId, NodeType},
    resources::{ImageRef, RawImage, RawImageData, RawImageFormat, RendererResources},
    styled_dom::StyledDom,
};
use azul_css::{AzString, U8Vec};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const SRC: &str = "https://example.org/logo.png";

const MAIL: &str = "<html><head></head><body><div>\
<img src=\"https://example.org/logo.png\" alt=\"Logo\"/>\
</div></body></html>";

/// A 40x20 picture, as the app decodes the bytes it fetched.
fn picture() -> ImageRef {
    let pixels: Vec<u8> = (0..40 * 20).flat_map(|_| [200u8, 30, 30, 255]).collect();
    ImageRef::new_rawimage(RawImage {
        pixels: RawImageData::U8(U8Vec::from_vec(pixels)),
        width: 40,
        height: 20,
        premultiplied_alpha: false,
        data_format: RawImageFormat::RGBA8,
        tag: U8Vec::from_vec(Vec::new()),
    })
    .expect("a raw image")
}

/// The mail laid out in an 800x600 window, with `cached` in its image cache.
fn laid_out(cached: Option<ImageRef>) -> LayoutWindow {
    let parsed = azul_layout::xml::parse_xml(MAIL).expect("the mail parses");
    let mut dom = azul_layout::xml::dom_from_parsed_xml(parsed);
    let styled_dom = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    if let Some(image) = cached {
        lw.image_cache.add_css_image_id(AzString::from(SRC), image);
    }
    let mut ws = FullWindowState::default();
    ws.size.dimensions = azul_core::geom::LogicalSize::new(800.0, 600.0);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled_dom, &ws, &rr, &sc, &mut dbg)
        .unwrap();
    lw
}

/// The img node's id.
fn img_node(lw: &LayoutWindow) -> NodeId {
    let result = &lw.layout_results[&DomId::ROOT_ID];
    let at = result
        .styled_dom
        .node_data
        .as_container()
        .iter()
        .position(|nd| matches!(nd.get_node_type(), NodeType::Image(_)))
        .expect("the <img> is an Image node");
    NodeId::new(at)
}

/// The sizes of the display list's image items.
fn painted_images(lw: &LayoutWindow) -> Vec<(f32, f32)> {
    lw.layout_results[&DomId::ROOT_ID]
        .display_list
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Image { bounds, .. } => {
                Some((bounds.0.size.width, bounds.0.size.height))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn an_img_whose_src_is_cached_lays_out_at_the_pictures_size() {
    let lw = laid_out(Some(picture()));
    let bounds = lw
        .get_node_bounds(DomId::ROOT_ID, img_node(&lw))
        .expect("the img has bounds");
    assert_eq!(
        (bounds.size.width, bounds.size.height),
        (40, 20),
        "the cached picture's natural size"
    );
}

#[test]
fn an_img_whose_src_is_cached_paints_the_picture() {
    let lw = laid_out(Some(picture()));
    assert_eq!(
        painted_images(&lw)
            .into_iter()
            .map(|(w, h)| (w.round(), h.round()))
            .collect::<Vec<_>>(),
        vec![(40.0, 20.0)]
    );
}

/// The display list's image items: (carries pixels, the placeholder's src).
fn image_items(lw: &LayoutWindow) -> Vec<(bool, Option<String>)> {
    lw.layout_results[&DomId::ROOT_ID]
        .display_list
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Image { image, .. } => Some((
                image.is_raw_image(),
                image.source_tag().map(str::to_string),
            )),
            _ => None,
        })
        .collect()
}

#[test]
fn an_img_whose_src_is_not_cached_keeps_its_src_for_a_renderer_that_fetches_it() {
    let lw = laid_out(None);
    let items = image_items(&lw);
    assert!(
        items
            .iter()
            .all(|(pixels, src)| !*pixels && src.as_deref() == Some(SRC)),
        "no pixels, only the placeholder naming its src: {items:?}"
    );
}

#[cfg(all(
    feature = "cpurender",
    feature = "text_layout",
    feature = "font_loading"
))]
#[test]
fn an_img_whose_src_is_not_cached_paints_nothing() {
    use azul_layout::cpurender::{render_dom_to_image, AzulPixmap};

    let parsed = azul_layout::xml::parse_xml(MAIL).expect("the mail parses");
    let dom = azul_layout::xml::dom_from_parsed_xml(parsed);
    let png = render_dom_to_image(dom, azul_css::css::Css::empty(), 800.0, 600.0, 1.0)
        .expect("the mail renders");
    let pixmap = AzulPixmap::decode_png(&png).expect("a png");
    // Inside the img's box (300x150 at the top left, with or without the
    // body's margin): the page shows through, white.
    let (x, y) = (150u32, 75u32);
    let at = ((y * pixmap.width() + x) * 4) as usize;
    let rgb = (pixmap.data()[at], pixmap.data()[at + 1], pixmap.data()[at + 2]);
    assert_eq!(
        rgb,
        (255, 255, 255),
        "a picture nobody fetched is not drawn (grey is the placeholder of an image that \
         exists but has no pixels yet)"
    );
}
