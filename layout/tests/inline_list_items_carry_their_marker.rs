//! `display: inline list-item` (CSS Display 3 s2.4): an inline box that is a
//! list item. It counts in its list's `list-item` counter and carries a
//! marker, which sits at the start of its inline content - a `::marker`
//! node made with the styled DOM, as the generated content is.

use azul_core::{dom::NodeType, id::NodeId, styled_dom::StyledDom};
use azul_css::props::layout::LayoutDisplay;

fn styled(markup: &str) -> StyledDom {
    let parsed = azul_layout::xml::parse_xml(markup).expect("parses");
    StyledDom::create_from_dom(azul_layout::xml::dom_from_parsed_xml(parsed))
}

fn all_of(styled: &StyledDom, ty: fn(&NodeType) -> bool) -> Vec<NodeId> {
    (0..styled.node_data.len())
        .map(NodeId::new)
        .filter(|id| ty(styled.node_data.as_container()[*id].get_node_type()))
        .collect()
}

fn first_child(styled: &StyledDom, node: NodeId) -> Option<NodeId> {
    styled.node_hierarchy.as_container()[node].first_child_id(node)
}

fn text_of(styled: &StyledDom, node: NodeId) -> String {
    match styled.node_data.as_container()[node].get_node_type() {
        NodeType::Text(s) => s.as_ref().as_str().to_string(),
        other => panic!("node {} is a {other:?}, not text", node.index()),
    }
}

/// The text of the `::marker` node `li` starts with, if it has one.
fn marker_text(styled: &StyledDom, li: NodeId) -> Option<String> {
    let marker = first_child(styled, li)?;
    if !matches!(
        styled.node_data.as_container()[marker].get_node_type(),
        NodeType::Marker
    ) {
        return None;
    }
    Some(text_of(styled, first_child(styled, marker)?))
}

#[test]
fn inline_list_item_parses_in_both_keyword_orders() {
    use azul_css::props::layout::parse_layout_display;
    assert_eq!(
        parse_layout_display("inline list-item"),
        Ok(LayoutDisplay::InlineListItem)
    );
    assert_eq!(
        parse_layout_display("list-item inline"),
        Ok(LayoutDisplay::InlineListItem)
    );
    assert_eq!(
        parse_layout_display("inline flow list-item"),
        Ok(LayoutDisplay::InlineListItem)
    );
    assert_eq!(parse_layout_display("block list-item"), Ok(LayoutDisplay::ListItem));
    assert_eq!(parse_layout_display("inline flow-root"), Ok(LayoutDisplay::InlineBlock));
    assert_eq!(parse_layout_display("block flex"), Ok(LayoutDisplay::Flex));
    assert!(LayoutDisplay::InlineListItem.is_inline_level());
}

#[test]
fn an_inline_list_item_starts_with_its_marker() {
    let styled = styled(
        r#"<style>
            li { display: inline list-item; }
        </style>
        <ol><li>a</li><li>b</li></ol>
        <ul><li>c</li></ul>"#,
    );
    let items = all_of(&styled, |t| matches!(t, NodeType::Li));
    assert_eq!(items.len(), 3);
    assert_eq!(
        azul_layout::solver3::getters::get_display_property(&styled, Some(items[0])).exact(),
        Some(LayoutDisplay::InlineListItem)
    );
    let markers: Vec<Option<String>> = items.iter().map(|li| marker_text(&styled, *li)).collect();
    assert_eq!(
        markers,
        [
            Some("1. ".to_string()),
            Some("2. ".to_string()),
            Some("\u{2022} ".to_string())
        ],
        "an ordered list numbers its items, an unordered one bullets them"
    );
}

#[test]
fn a_block_list_item_leaves_its_marker_to_the_layout() {
    // A block list item's marker sits outside its box; the layout makes it.
    let styled = styled(r#"<ol><li>a</li></ol>"#);
    let items = all_of(&styled, |t| matches!(t, NodeType::Li));
    assert_eq!(marker_text(&styled, items[0]), None);
}

#[test]
fn an_inline_list_item_counts_for_generated_content() {
    let styled = styled(
        r#"<style>
            li { display: inline list-item; list-style-type: none; }
            li::after { content: "(" counter(list-item) ")"; }
        </style>
        <ol><li>a</li><li>b</li></ol>"#,
    );
    let afters = all_of(&styled, |t| matches!(t, NodeType::After));
    let texts: Vec<String> = afters
        .iter()
        .map(|a| text_of(&styled, first_child(&styled, *a).expect("text")))
        .collect();
    assert_eq!(texts, ["(1)", "(2)"]);
    let items = all_of(&styled, |t| matches!(t, NodeType::Li));
    assert_eq!(
        marker_text(&styled, items[0]).as_deref(),
        Some(""),
        "list-style-type: none writes no marker"
    );
}

/// A marker's numbers are tabular (CSS Lists 3's UA sheet: `::marker {
/// font-variant-numeric: tabular-nums }`), so the numbers of a list line up:
/// the inline marker node resolves it, whatever its item says.
#[test]
fn an_inline_list_items_marker_has_tabular_numbers() {
    use azul_css::props::property::{CssProperty, CssPropertyType};
    let styled = styled(
        r#"<style>li { display: inline list-item; font-variant-numeric: oldstyle-nums; }</style>
        <ol><li>a</li></ol>"#,
    );
    let li = all_of(&styled, |t| matches!(t, NodeType::Li))[0];
    let marker = first_child(&styled, li).expect("the marker");
    let state = styled.styled_nodes.as_container()[marker].styled_node_state;
    let nodes = styled.node_data.as_container();
    let numeric = styled.css_property_cache.ptr.get_property(
        &nodes[marker],
        &marker,
        &state,
        &CssPropertyType::FontVariantNumeric,
    );
    let Some(CssProperty::FontVariantNumeric(v)) = numeric else {
        panic!("the marker resolves font-variant-numeric, got {numeric:?}");
    };
    let v = v.get_property().copied().expect("a value");
    assert!(v.tabular_nums && !v.oldstyle_nums, "{v:?}");
}
