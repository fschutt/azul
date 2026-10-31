//! A presentational attribute (`<svg width="100">`, `<img width>`, an SVG
//! `<text>`'s `font-size`) is an author-level rule of specificity 0 (CSS 2.2
//! 6.4.4, HTML "presentational hints"): every stylesheet rule and inline style
//! beats it. The loaders stored the hints as the node's INLINE style, so an
//! `<svg>` kept its markup size whatever CSS said - AzPdf's pages stayed at
//! 1 px per point at every zoom.

use azul_core::{
    dom::{Dom, DomId, NodeType},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// The used size of the first `<svg>` of `body`.
fn svg_size(mut body: Dom) -> (f32, f32) {
    let styled = StyledDom::create(&mut body, azul_css::css::Css::empty());
    laid_out_svg_size(styled)
}

fn laid_out_svg_size(styled: StyledDom) -> (f32, f32) {
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .unwrap();
    let lr = lw.get_layout_result(&DomId::ROOT_ID).unwrap();
    let nodes = lr.styled_dom.node_data.as_container();
    let node = lr
        .layout_tree
        .nodes
        .iter()
        .find(|n| {
            n.dom_node_id
                .is_some_and(|d| matches!(nodes[d].get_node_type(), NodeType::Svg))
        })
        .expect("an svg box");
    let size = node.used_size.expect("a size");
    (size.width, size.height)
}

fn svg_fragment(markup: &str) -> Dom {
    let xml = azul_layout::xml::parse_xml(markup).expect("the markup parses");
    azul_layout::xml::dom_fragment_from_parsed_xml(xml)
}

#[test]
fn an_svgs_css_size_beats_its_width_and_height_attributes() {
    let svg = svg_fragment(r#"<svg width="100px" height="50px" viewBox="0 0 100 50"></svg>"#)
        .with_css("display: block; width: 300px; height: 150px;");
    let body = Dom::create_body().with_css("margin: 0px").with_child(svg);
    assert_eq!(svg_size(body), (300.0, 150.0), "the author's CSS size, not the attributes'");
}

#[test]
fn a_stylesheet_rule_beats_a_presentational_attribute_but_not_a_style_attribute() {
    let markup = r#"<html><head><style>svg { width: 300px; height: 150px; }</style></head>
<body style="margin: 0px"><svg width="100" height="50" viewBox="0 0 100 50"></svg></body></html>"#;
    let styled = azul_layout::xml::parse_xml_to_styled_dom(markup).expect("parses");
    assert_eq!(laid_out_svg_size(styled), (300.0, 150.0), "the stylesheet wins over the hints");

    let inline = r#"<html><head><style>svg { width: 300px; height: 150px; }</style></head>
<body style="margin: 0px"><svg width="100" height="50" viewBox="0 0 100 50" style="width: 120px; height: 60px"></svg></body></html>"#;
    let styled = azul_layout::xml::parse_xml_to_styled_dom(inline).expect("parses");
    assert_eq!(laid_out_svg_size(styled), (120.0, 60.0), "a style attribute wins over both");

    let bare = r#"<html><body style="margin: 0px"><svg width="100" height="50" viewBox="0 0 100 50"></svg></body></html>"#;
    let styled = azul_layout::xml::parse_xml_to_styled_dom(bare).expect("parses");
    assert_eq!(laid_out_svg_size(styled), (100.0, 50.0), "without CSS the attributes size it");
}
