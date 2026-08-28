#[cfg(test)]
mod extract_tests {
    use super::*;
    use crate::{
        dom::{Dom, NodeData, NodeType},
        styled_dom::StyledDom,
    };

    /// The extracted DOM holds exactly the popup's content, re-rooted as a
    /// plain block, and nothing from outside the subtree.
    #[test]
    fn extracts_exactly_the_subtree_re_rooted_as_a_div() {
        let popup = Dom::create_from_data(NodeData::create_node(NodeType::TransientWindow(
            TransientWindowConfig::opened(),
        )))
        .with_child(Dom::create_p_with_text("inside"));
        let full = Dom::create_body()
            .with_child(Dom::create_p_with_text("outside"))
            .with_child(Dom::create_div().with_child(popup));
        let styled = StyledDom::create_from_dom(full);

        // body=0, p=1, text=2, div=3, transient=4, p=5, text=6
        let nodes = styled.node_data.as_container();
        let tw = nodes
            .linear_iter()
            .find(|n| {
                matches!(
                    nodes.get(*n).map(NodeData::get_node_type),
                    Some(NodeType::TransientWindow(_))
                )
            })
            .expect("the transient node");

        let out = extract_subtree_as_dom(&styled, tw).expect("extracts");
        assert!(
            matches!(out.root.get_node_type(), NodeType::Div),
            "the popup's own root must be a plain container in its window"
        );
        // root + p + text = 3 nodes; "outside" must not be among them.
        let out_styled = StyledDom::create_from_dom(out);
        let texts: Vec<String> = out_styled
            .node_data
            .as_ref()
            .iter()
            .filter_map(|n| match n.get_node_type() {
                NodeType::Text(t) => Some(t.as_str().to_string()),
                _ => None,
            })
            .collect();
        assert_eq!(texts, vec!["inside".to_string()]);
    }

    /// Only a TransientWindow can be extracted - asking for a div is a caller
    /// bug and must not quietly open a window onto arbitrary content.
    #[test]
    fn refuses_a_non_transient_root() {
        let styled = StyledDom::create_from_dom(Dom::create_body().with_child(Dom::create_div()));
        assert!(extract_subtree_as_dom(&styled, crate::id::NodeId::new(1)).is_none());
    }

    /// Author CSS attached with `Dom::with_css` is SCOPED - it is consumed
    /// into the property cache when the tree is styled - so cloning node data
    /// alone would drop it. The extracted copy must carry the resolved style,
    /// including what the root inherited from ancestors it leaves behind.
    #[test]
    fn resolved_style_travels_with_the_extracted_subtree() {
        use azul_css::props::{
            basic::{ColorU, PixelValue},
            layout::LayoutWidth,
            property::{CssProperty, CssPropertyType},
            style::StyleTextColor,
        };

        let popup = Dom::create_from_data(NodeData::create_node(NodeType::TransientWindow(
            TransientWindowConfig::opened(),
        )))
        .with_child(Dom::create_div().with_css("width: 240px;"));
        let full = Dom::create_body()
            .with_css("color: #123456;") // inherited by the popup root from OUTSIDE the subtree
            .with_child(Dom::create_div().with_child(popup));
        let styled = StyledDom::create_from_dom(full);

        let nodes = styled.node_data.as_container();
        let tw = nodes
            .linear_iter()
            .find(|n| {
                matches!(
                    nodes.get(*n).map(NodeData::get_node_type),
                    Some(NodeType::TransientWindow(_))
                )
            })
            .expect("the transient node");
        let out = extract_subtree_as_dom(&styled, tw).expect("extracts");

        // The popup's root inherited the body's colour.
        let root_color = out
            .root
            .style
            .iter_inline_properties()
            .find_map(|(p, _)| match p {
                CssProperty::TextColor(c) => c.get_property().copied(),
                _ => None,
            });
        assert_eq!(
            root_color,
            Some(StyleTextColor {
                inner: ColorU {
                    r: 0x12,
                    g: 0x34,
                    b: 0x56,
                    a: 255
                }
            }),
            "the root must carry what it inherited from outside the subtree"
        );

        // The child's own `width: 240px` came along as a matched author rule.
        let child = out.children.as_ref().first().expect("the sized child");
        let width = child
            .root
            .style
            .iter_inline_properties()
            .find_map(|(p, _)| match p {
                CssProperty::Width(w) => w.get_property().cloned(),
                _ => None,
            });
        assert_eq!(
            width,
            Some(LayoutWidth::Px(PixelValue::px(240.0))),
            "a scoped `with_css` rule must survive extraction"
        );
        assert!(
            child
                .root
                .style
                .iter_inline_properties()
                .all(|(p, _)| p.get_type() != CssPropertyType::TextColor),
            "a non-root node carries only its OWN matched rules; inheritance is re-derived"
        );
    }
}
