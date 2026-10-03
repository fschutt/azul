//! The builtin HTML elements take their presentational arguments.
//!
//! The user's ruling: components take their declared arguments. MAILVIEW
//! made `a` / `area` / `link` / `base` / `img` land `href` / `src` / `alt`;
//! its audit listed what mail still writes and nothing reads:
//!
//! - `<ol start type reversed>`, `<li value type>`, `<ul type>`: the numbering always started at
//!   1, counted up and followed the UA `decimal`;
//! - `<font face size color>`: a `<font>` was a bare span;
//! - `<center>`, `align` on `div` / `p` / the headings: nothing was centred;
//! - `<img align border hspace>` and a percentage `width`: the image neither floated nor got its
//!   border or its gap;
//! - `<body bgcolor text>`.
//!
//! Each is the element's argument, declared in its builtin data model and
//! landed as the HTML Standard's rendering section maps it (the
//! presentational hints, before the element's own `style` - which wins).
//! Table attributes are TABLE-A's.
//!
//! Both loaders are checked: the lenient document loader
//! (`parse_html_to_styled_dom`) and the lenient tree loader (`parse_html` +
//! `dom_from_parsed_xml`), which land the arguments in two places.
//!
//! Not compiled by the author (house rule); RED before the fix.

use azul_core::{
    dom::{DomId, NodeId, NodeType},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::props::property::{CssProperty, CssPropertyType};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    window::LayoutWindow,
    window_state::FullWindowState,
    xml::{dom_from_parsed_xml, parse_html, parse_html_to_styled_dom},
};
use rust_fontconfig::FcFontCache;

/// The value `declaration` (`color: #003366`) has, printed as the cascade
/// prints the property's value.
fn css(declaration: &str) -> String {
    let map = azul_css::props::property::get_css_key_map();
    let parsed = azul_core::xml::attributes::style_declarations(declaration, &map);
    let property: &CssProperty = &parsed
        .first()
        .unwrap_or_else(|| panic!("`{declaration}` parses"))
        .property;
    property.value()
}

/// The two loaders' `StyledDom`s of `html`.
fn both_loaders(html: &str) -> [(&'static str, StyledDom); 2] {
    [
        ("document loader", parse_html_to_styled_dom(html)),
        (
            "tree loader",
            StyledDom::create_from_dom(dom_from_parsed_xml(parse_html(html))),
        ),
    ]
}

/// The nodes of type `node_type`, in document order.
fn nodes_of(styled: &StyledDom, node_type: &NodeType) -> Vec<NodeId> {
    styled
        .node_data
        .as_ref()
        .iter()
        .enumerate()
        .filter(|(_, n)| {
            core::mem::discriminant(&n.node_type) == core::mem::discriminant(node_type)
        })
        .map(|(i, _)| NodeId::new(i))
        .collect()
}

/// The cascaded value of `property` on `node`, printed.
fn value_of(styled: &StyledDom, node: NodeId, property: CssPropertyType) -> Option<String> {
    let node_data = &styled.node_data.as_container()[node];
    let state = &styled.styled_nodes.as_container()[node].styled_node_state;
    styled
        .css_property_cache
        .ptr
        .get_property(node_data, &node, state, &property)
        .map(CssProperty::value)
}

fn assert_value(
    loader: &str,
    styled: &StyledDom,
    node: NodeId,
    property: CssPropertyType,
    declaration: &str,
) {
    assert_eq!(
        value_of(styled, node, property),
        Some(css(declaration)),
        "{loader}: node {node:?} should have `{declaration}`"
    );
}

#[test]
fn list_type_arguments_set_the_marker_style() {
    let html = "<ol type=\"a\"><li>x</li></ol><ol type=\"I\"><li>y</li></ol>\
                <ul type=\"square\"><li>z</li></ul><ol><li type=\"i\">w</li></ol>";
    for (loader, styled) in both_loaders(html) {
        let ols = nodes_of(&styled, &NodeType::Ol);
        let uls = nodes_of(&styled, &NodeType::Ul);
        let lis = nodes_of(&styled, &NodeType::Li);
        assert_eq!((ols.len(), uls.len(), lis.len()), (3, 1, 4), "{loader}");
        let t = CssPropertyType::ListStyleType;
        assert_value(loader, &styled, ols[0], t, "list-style-type: lower-alpha");
        assert_value(loader, &styled, ols[1], t, "list-style-type: upper-roman");
        assert_value(loader, &styled, uls[0], t, "list-style-type: square");
        assert_value(loader, &styled, lis[3], t, "list-style-type: lower-roman");
    }
}

#[test]
fn font_face_size_and_color_style_their_text() {
    let html = "<p><font face=\"Verdana\" size=\"5\" color=\"#003366\">a</font>\
                <font size=\"+1\" color=\"RED\">b</font>\
                <font size=\"1\" color=\"red\" style=\"color: blue\">c</font></p>";
    for (loader, styled) in both_loaders(html) {
        let fonts = nodes_of(&styled, &NodeType::Span);
        assert_eq!(fonts.len(), 3, "{loader}: a <font> is a span");
        assert_value(
            loader,
            &styled,
            fonts[0],
            CssPropertyType::FontFamily,
            "font-family: Verdana",
        );
        // The legacy sizes 1..7 are x-small .. xxx-large (10 13 16 18 24 32 48 px);
        // `+1` is relative to 3.
        assert_value(
            loader,
            &styled,
            fonts[0],
            CssPropertyType::FontSize,
            "font-size: 24px",
        );
        assert_value(
            loader,
            &styled,
            fonts[0],
            CssPropertyType::TextColor,
            "color: #003366",
        );
        assert_value(
            loader,
            &styled,
            fonts[1],
            CssPropertyType::FontSize,
            "font-size: 18px",
        );
        assert_value(
            loader,
            &styled,
            fonts[1],
            CssPropertyType::TextColor,
            "color: red",
        );
        assert_value(
            loader,
            &styled,
            fonts[2],
            CssPropertyType::FontSize,
            "font-size: 10px",
        );
        // The element's own style wins over its arguments.
        assert_value(
            loader,
            &styled,
            fonts[2],
            CssPropertyType::TextColor,
            "color: blue",
        );
    }
}

#[test]
fn center_and_align_set_the_text_alignment() {
    let html = "<center>a</center><div align=\"right\">b</div><p align=\"center\">c</p>\
                <h1 align=\"justify\">d</h1>";
    for (loader, styled) in both_loaders(html) {
        let divs = nodes_of(&styled, &NodeType::Div);
        let p = nodes_of(&styled, &NodeType::P)[0];
        let h1 = nodes_of(&styled, &NodeType::H1)[0];
        // `<center>` is read as a div.
        assert_eq!(divs.len(), 2, "{loader}");
        let t = CssPropertyType::TextAlign;
        assert_value(loader, &styled, divs[0], t, "text-align: center");
        assert_value(loader, &styled, divs[1], t, "text-align: right");
        assert_value(loader, &styled, p, t, "text-align: center");
        assert_value(loader, &styled, h1, t, "text-align: justify");
    }
}

#[test]
fn an_image_floats_frames_and_spaces_by_its_arguments() {
    let html = "<p><img src=\"cid:a\" width=\"100%\" align=\"left\" border=\"2\" hspace=\"4\" \
                vspace=\"3\"><img src=\"cid:b\" align=\"middle\"></p>";
    for (loader, styled) in both_loaders(html) {
        let imgs: Vec<NodeId> = styled
            .node_data
            .as_ref()
            .iter()
            .enumerate()
            .filter(|(_, n)| matches!(n.node_type, NodeType::Image(_)))
            .map(|(i, _)| NodeId::new(i))
            .collect();
        assert_eq!(imgs.len(), 2, "{loader}");
        assert_value(
            loader,
            &styled,
            imgs[0],
            CssPropertyType::Width,
            "width: 100%",
        );
        assert_value(
            loader,
            &styled,
            imgs[0],
            CssPropertyType::Float,
            "float: left",
        );
        assert_value(
            loader,
            &styled,
            imgs[0],
            CssPropertyType::BorderLeftWidth,
            "border-left-width: 2px",
        );
        assert_value(
            loader,
            &styled,
            imgs[0],
            CssPropertyType::MarginLeft,
            "margin-left: 4px",
        );
        assert_value(
            loader,
            &styled,
            imgs[0],
            CssPropertyType::MarginTop,
            "margin-top: 3px",
        );
        assert_value(
            loader,
            &styled,
            imgs[1],
            CssPropertyType::VerticalAlign,
            "vertical-align: middle",
        );
    }
}

#[test]
fn the_body_takes_its_background_and_text_colour() {
    let html = "<html><body bgcolor=\"#ffeedd\" text=\"#333333\"><p>x</p></body></html>";
    for (loader, styled) in both_loaders(html) {
        let body = nodes_of(&styled, &NodeType::Body)[0];
        assert_value(
            loader,
            &styled,
            body,
            CssPropertyType::TextColor,
            "color: #333333",
        );
        assert_value(
            loader,
            &styled,
            body,
            CssPropertyType::BackgroundContent,
            "background-color: #ffeedd",
        );
    }
}

/// The `list-item` counter of every `<li>`, in document order, after a
/// layout.
fn list_numbers(html: &str) -> Vec<i32> {
    let styled = StyledDom::create_from_dom(dom_from_parsed_xml(parse_html(html)));
    let lis: Vec<NodeId> = nodes_of(&styled, &NodeType::Li);
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 600.0);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled, &ws, &rr, &sc, &mut dbg)
        .expect("the lists lay out");
    let _ = lw.get_layout_result(&DomId::ROOT_ID).expect("laid out");
    let tree = lw.layout_cache.tree.as_ref().expect("a layout tree");
    lis.iter()
        .map(|li| {
            let layout = tree
                .dom_to_layout
                .get(li)
                .and_then(|v| v.first())
                .expect("laid out");
            *lw.layout_cache
                .counters
                .get(&(layout.index(), "list-item".to_string()))
                .unwrap_or(&i32::MIN)
        })
        .collect()
}

#[test]
fn ordered_lists_number_from_start_down_when_reversed_and_from_a_value() {
    let html = "<ol start=\"5\"><li>a</li><li>b</li></ol>\
                <ol reversed><li>a</li><li>b</li><li>c</li></ol>\
                <ol reversed start=\"10\"><li>a</li><li>b</li></ol>\
                <ol><li>a</li><li value=\"7\">b</li><li>c</li></ol>";
    assert_eq!(list_numbers(html), vec![5, 6, 3, 2, 1, 10, 9, 1, 7, 8]);
}

/// CSS Lists 3 s4.4.2 (what Chrome numbers; WPT css-lists/counter-list-item):
/// a `reversed` list without `start` begins so that the items before its
/// first `<li value>` count down INTO that value - not at its item count.
/// azul began `<ol reversed>` + `<li value="30">` at 6 (six items).
#[test]
fn a_reversed_list_without_start_counts_down_into_its_first_value() {
    let html = "<ol reversed><li>a</li><li>b</li><li value=\"30\">c</li><li>d</li>\
                <li value=\"35\">e</li><li>f</li></ol>";
    assert_eq!(list_numbers(html), vec![32, 31, 30, 29, 35, 34]);
}
