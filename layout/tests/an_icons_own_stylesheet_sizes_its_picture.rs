//! An icon's own stylesheet sizes its picture.
//!
//! An icon drawn as a picture becomes an `<img>` with a default size - the
//! picture's natural size, or `1em` - where the icon states none. That default
//! is what HTML's `width` / `height` attributes are to an `<img>`: a
//! presentational hint, which every author rule beats (CSS 2.2 §6.4.4). It
//! was written as INLINE style, which beats every stylesheet - and
//! `Dom::with_css` is a stylesheet scoped to the node, not inline style - so a
//! button face styled `width: 40px; height: 40px` on its icon shrank to the
//! picture's 16 px once the icon pack drew that name as a picture (AzPlayer's
//! back button under flora's Haiku icons).
//!
//! Not compiled by the author (house rule).

#![cfg(all(feature = "cpurender", feature = "text_layout", feature = "font_loading"))]

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::{LogicalRect, LogicalSize},
    icon::{resolve_icons_in_dom, SharedIconProvider},
    resources::{ImageRef, RendererResources},
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_css::system::SystemStyle;
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    cpurender::AzulPixmap,
    icon::{create_default_icon_provider, register_image_icon},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// A 16 x 16 opaque picture.
fn picture() -> ImageRef {
    let mut pm = AzulPixmap::new(16, 16).expect("pixmap");
    pm.fill(0, 0, 0, 255);
    let raw = azul_core::resources::RawImage {
        pixels: azul_core::resources::RawImageData::U8(pm.data().to_vec().into()),
        width: 16,
        height: 16,
        premultiplied_alpha: false,
        data_format: azul_core::resources::RawImageFormat::RGBA8,
        tag: Vec::new().into(),
    };
    ImageRef::new_rawimage(raw).expect("an image")
}

/// The rect of the icon `#icon` styled `icon_css` (through `with_css`) in a
/// flex row, after the icon resolved to the picture.
fn icon_rect(icon_css: &str) -> LogicalRect {
    let mut provider = create_default_icon_provider();
    register_image_icon(&mut provider, "app", "glyph", picture());
    let shared = SharedIconProvider::from_handle(provider);
    let mut dom = Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_div()
            .with_css("display: flex; align-items: flex-start;")
            .with_child(
                Dom::create_icon("glyph")
                    .with_id("icon".into())
                    .with_css(icon_css),
            ),
    );
    resolve_icons_in_dom(&mut dom, &shared, &SystemStyle::default());

    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(300.0, 200.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the row lays out");
    let result = lw.get_layout_result(&DomId::ROOT_ID).expect("laid out");
    let index = result
        .styled_dom
        .node_data
        .as_container()
        .internal
        .iter()
        .position(|n| n.has_id("icon"))
        .expect("the resolved icon keeps its id");
    lw.get_node_layout_rect(DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
    })
    .expect("the icon has a rect")
}

#[test]
fn an_icon_styled_forty_pixels_square_is_forty_pixels_square_as_a_picture() {
    let rect = icon_rect("width: 40px; height: 40px;");
    assert!(
        (rect.size.width - 40.0).abs() < 0.5 && (rect.size.height - 40.0).abs() < 0.5,
        "the icon's stylesheet beats the picture's default size, got {:?}",
        rect.size
    );
}

#[test]
fn an_icon_that_states_no_size_is_its_pictures_natural_size() {
    let rect = icon_rect("display: block;");
    assert!(
        (rect.size.width - 16.0).abs() < 0.5 && (rect.size.height - 16.0).abs() < 0.5,
        "without a size of its own the picture keeps its natural 16 x 16, got {:?}",
        rect.size
    );
}
