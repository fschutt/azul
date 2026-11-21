//! A picture keeps its own proportions when CSS sizes it down or gives it only
//! one of its two sizes (CSS 2.2 10.3.2, 10.4 and 10.6.2).
//!
//! The expected sizes are Chrome's for the same markup (an `<img>` of a
//! 256x192 or 192x256 SVG): a file manager's list draws each picture in an
//! icon-sized cell as `max-width: 16px; max-height: 16px`, and Chrome shows a
//! landscape photo 16x12 and a portrait one 12x16. A sliver one or two pixels
//! wide means the min/max table was handed a height that does not belong to
//! the width beside it.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId, NodeType},
    geom::LogicalSize,
    resources::{ImageRef, RawImageFormat, RendererResources},
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// A decoded picture of `width` x `height` pixels (its pixels do not matter here).
fn picture(width: usize, height: usize) -> ImageRef {
    ImageRef::null_image(width, height, RawImageFormat::RGBA8, Vec::new())
}

/// Lays `dom` out in a 400x300 window and answers the size of every picture in it, in DOM order.
fn picture_sizes(dom: Dom) -> Vec<(f32, f32)> {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let styled = StyledDom::create_from_dom(dom);
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = None;
    lw.layout_and_generate_display_list(styled, &ws, &rr, &sc, &mut dbg)
        .expect("lays out");
    let sd = &lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("laid out")
        .styled_dom;
    let pictures: Vec<usize> = sd
        .node_data
        .as_ref()
        .iter()
        .enumerate()
        .filter(|(_, nd)| matches!(nd.get_node_type(), NodeType::Image(_)))
        .map(|(i, _)| i)
        .collect();
    pictures
        .into_iter()
        .map(|i| {
            let rect = lw
                .get_node_layout_rect(DomNodeId {
                    dom: DomId::ROOT_ID,
                    node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(i))),
                })
                .expect("the picture has a rect");
            (rect.size.width, rect.size.height)
        })
        .collect()
}

/// The icon-sized cell a file list draws a picture in: the picture centred in a 16 px square.
fn icon_cell(image: ImageRef) -> Dom {
    Dom::create_div()
        .with_css(
            "display: flex; flex-direction: row; align-items: center; justify-content: center; \
             width: 16px; height: 16px; margin-right: 6px; flex-shrink: 0;",
        )
        .with_child(Dom::create_image(image).with_css("max-width: 16px; max-height: 16px;"))
}

fn assert_size(got: (f32, f32), want: (f32, f32), what: &str) {
    assert!(
        (got.0 - want.0).abs() <= 0.5 && (got.1 - want.1).abs() <= 0.5,
        "{what}: Chrome draws it {}x{}, got {}x{}",
        want.0,
        want.1,
        got.0,
        got.1
    );
}

#[test]
fn a_landscape_picture_in_an_icon_cell_is_scaled_down_whole() {
    let sizes = picture_sizes(Dom::create_body().with_child(icon_cell(picture(256, 192))));
    assert_size(
        sizes[0],
        (16.0, 12.0),
        "a 256x192 picture under max-width/max-height 16px",
    );
}

#[test]
fn a_portrait_picture_in_an_icon_cell_is_scaled_down_whole() {
    let sizes = picture_sizes(Dom::create_body().with_child(icon_cell(picture(192, 256))));
    assert_size(
        sizes[0],
        (12.0, 16.0),
        "a 192x256 picture under max-width/max-height 16px",
    );
}

#[test]
fn a_picture_in_a_list_row_beside_its_name_is_scaled_down_whole() {
    let row = Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; min-width: 0px; width: 400px;")
        .with_child(icon_cell(picture(192, 256)))
        .with_child(Dom::create_div_with_text("holiday.jpg"));
    let sizes = picture_sizes(Dom::create_body().with_child(row));
    assert_size(sizes[0], (12.0, 16.0), "the picture of a list row");
}

#[test]
fn a_block_picture_capped_on_both_sides_is_scaled_down_whole() {
    let dom = Dom::create_body().with_child(
        Dom::create_div().with_css("width: 300px;").with_child(
            Dom::create_image(picture(256, 192))
                .with_css("display: block; max-width: 16px; max-height: 16px;"),
        ),
    );
    let sizes = picture_sizes(dom);
    assert_size(
        sizes[0],
        (16.0, 12.0),
        "a block picture under max-width/max-height 16px",
    );
}

#[test]
fn a_picture_given_only_a_width_takes_the_height_of_its_ratio() {
    let dom =
        Dom::create_body().with_child(Dom::create_div().with_css("width: 300px;").with_child(
            Dom::create_image(picture(256, 192)).with_css("display: block; width: 64px;"),
        ));
    let sizes = picture_sizes(dom);
    assert_size(sizes[0], (64.0, 48.0), "a 256x192 picture 64px wide");
}

#[test]
fn a_picture_given_only_a_height_takes_the_width_of_its_ratio() {
    let dom =
        Dom::create_body().with_child(Dom::create_div().with_css("width: 300px;").with_child(
            Dom::create_image(picture(256, 192)).with_css("display: block; height: 48px;"),
        ));
    let sizes = picture_sizes(dom);
    assert_size(sizes[0], (64.0, 48.0), "a 256x192 picture 48px tall");
}
