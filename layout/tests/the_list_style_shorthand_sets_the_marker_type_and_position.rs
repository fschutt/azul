//! The `list-style` shorthand sets the marker's type and position.
//!
//! MAILVIEW's audit found `list-style-position: inside` "not honoured, the
//! markers stay outside". The longhand reaches the marker (`fc.rs` reads it
//! per list item, REFCI's WPT runs paint inside markers inline); what never
//! reached anything is the SHORTHAND: the CSS parser had no `list-style` at
//! all, so `list-style: inside`, `list-style: lower-roman inside` and - the
//! one every newsletter's menu writes - `list-style: none` were dropped with
//! a parse warning, and the menu got bullets.
//!
//! CSS Lists 3: `list-style` is `<type> || <position> || <image>`, an
//! omitted one reset to its initial value (`disc`, `outside`); `none` is the
//! type.
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
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState, xml::parse_html_to_styled_dom,
};
use rust_fontconfig::FcFontCache;

fn css(declaration: &str) -> String {
    let map = azul_css::props::property::get_css_key_map();
    let parsed = azul_core::xml::attributes::style_declarations(declaration, &map);
    parsed
        .first()
        .unwrap_or_else(|| panic!("`{declaration}` parses"))
        .property
        .value()
}

fn value_of(styled: &StyledDom, node: NodeId, property: CssPropertyType) -> Option<String> {
    let node_data = &styled.node_data.as_container()[node];
    let state = &styled.styled_nodes.as_container()[node].styled_node_state;
    styled
        .css_property_cache
        .ptr
        .get_property(node_data, &node, state, &property)
        .map(CssProperty::value)
}

fn lists(styled: &StyledDom) -> Vec<NodeId> {
    styled
        .node_data
        .as_ref()
        .iter()
        .enumerate()
        .filter(|(_, n)| matches!(n.node_type, NodeType::Ol | NodeType::Ul))
        .map(|(i, _)| NodeId::new(i))
        .collect()
}

#[test]
fn the_shorthand_expands_to_the_type_and_the_position() {
    let styled = parse_html_to_styled_dom(
        "<ul style=\"list-style: none\"><li>a</li></ul>\
         <ol style=\"list-style: lower-roman inside\"><li>b</li></ol>\
         <ol style=\"list-style: inside\"><li>c</li></ol>",
    );
    let l = lists(&styled);
    assert_eq!(l.len(), 3);
    let (ty, pos) = (
        CssPropertyType::ListStyleType,
        CssPropertyType::ListStylePosition,
    );
    assert_eq!(
        value_of(&styled, l[0], ty),
        Some(css("list-style-type: none"))
    );
    assert_eq!(
        value_of(&styled, l[1], ty),
        Some(css("list-style-type: lower-roman"))
    );
    assert_eq!(
        value_of(&styled, l[1], pos),
        Some(css("list-style-position: inside"))
    );
    // An omitted type is reset to its initial value.
    assert_eq!(
        value_of(&styled, l[2], ty),
        Some(css("list-style-type: disc"))
    );
    assert_eq!(
        value_of(&styled, l[2], pos),
        Some(css("list-style-position: inside"))
    );
}

/// The smallest x of any glyph the page paints.
fn leftmost_glyph(html: &str) -> f32 {
    let styled = parse_html_to_styled_dom(html);
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled, &ws, &rr, &sc, &mut dbg)
        .expect("the list lays out");
    let result = lw.get_layout_result(&DomId::ROOT_ID).expect("laid out");
    result
        .display_list
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Text { glyphs, .. } => {
                glyphs.iter().map(|g| g.point.x).reduce(f32::min)
            }
            _ => None,
        })
        .fold(f32::MAX, f32::min)
}

#[test]
fn an_inside_marker_sits_in_the_items_content_box() {
    // No margins, no padding: an OUTSIDE marker would hang left of x = 0.
    for list in [
        "<ol style=\"list-style-position: inside; margin: 0; padding: 0\">",
        "<ol style=\"list-style: inside; margin: 0; padding: 0\">",
    ] {
        let html = format!(
            "<html><body style=\"margin: 0\">{list}<li>One</li><li>Two</li></ol></body></html>"
        );
        let x = leftmost_glyph(&html);
        assert!(
            x > -0.5 && x < 400.0,
            "{list}: the leftmost glyph is at x = {x}, outside the list's box"
        );
    }
}
