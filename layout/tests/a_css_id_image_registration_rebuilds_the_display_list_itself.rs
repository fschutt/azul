//! Registering an image under a CSS id rebuilds the display list itself -
//! the window shows it with no further call, on every backend.
//!
//! `background-image: url(id)` resolves at display-list build time, so a
//! registration (`CallbackInfo::add_image_to_cache`, `ContentChange::
//! ImageById`) must rebuild the list. `apply_content_change` answered
//! `RebuildDisplayList` for it WITHOUT rebuilding, unlike its clip-mask and
//! node-style arms, which rebuild and answer the same tier. The tier means
//! "the list was rebuilt, send it": the dll marks the list dirty, and a GPU
//! backend that resends a dirty list (X11) sent the stale one - the image
//! stayed invisible until something else rebuilt it (HEADLESS6 fixed the
//! headless backend by rebuilding there; the root is here).

use azul_core::{
    dom::{Dom, DomId},
    geom::LogicalSize,
    resources::{ImageRef, RawImage, RawImageData, RawImageFormat, RendererResources},
    styled_dom::StyledDom,
};
use azul_css::{AzString, U8Vec};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    overlay::{ContentChange, ContentDirtyTier},
    solver3::display_list::DisplayListItem,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const ID: &str = "live-logo";

fn picture() -> ImageRef {
    let pixels: Vec<u8> = (0..16 * 16).flat_map(|_| [200u8, 30, 30, 255]).collect();
    ImageRef::new_rawimage(RawImage {
        pixels: RawImageData::U8(U8Vec::from_vec(pixels)),
        width: 16,
        height: 16,
        premultiplied_alpha: false,
        data_format: RawImageFormat::RGBA8,
        tag: U8Vec::from_vec(Vec::new()),
    })
    .expect("a raw image")
}

fn laid_out() -> LayoutWindow {
    let page = Dom::create_body().with_child(Dom::create_div().with_css(
        "width: 80px; height: 60px; background: blue; background-image: url(\"live-logo\");",
    ));
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    let mut debug = None;
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(page),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut debug,
    )
    .expect("the page lays out");
    lw
}

fn paints_an_image(lw: &LayoutWindow) -> bool {
    lw.layout_results[&DomId::ROOT_ID]
        .display_list
        .items
        .iter()
        .any(|item| matches!(item, DisplayListItem::Image { .. }))
}

#[test]
fn a_css_id_image_registration_rebuilds_the_display_list_itself() {
    let mut lw = laid_out();
    assert!(!paints_an_image(&lw), "harness: no image under the id yet");
    let result = lw.apply_content_change(ContentChange::ImageById {
        id: AzString::from(ID),
        image: Some(picture()),
    });
    assert_eq!(result.tier, ContentDirtyTier::RebuildDisplayList);
    assert!(
        paints_an_image(&lw),
        "the tier says the list was rebuilt: it must paint the image now"
    );
}

#[test]
fn removing_a_css_id_image_rebuilds_the_display_list_itself() {
    let mut lw = laid_out();
    lw.apply_content_change(ContentChange::ImageById {
        id: AzString::from(ID),
        image: Some(picture()),
    });
    let result = lw.apply_content_change(ContentChange::ImageById {
        id: AzString::from(ID),
        image: None,
    });
    assert_eq!(result.tier, ContentDirtyTier::RebuildDisplayList);
    assert!(
        !paints_an_image(&lw),
        "the removed image is gone from the list at once"
    );
}
