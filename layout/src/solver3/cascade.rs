//! solver3/cascade.rs
//!
//! Implements CSS cascading and inheritance by walking the layout tree.

use crate::solver3::{layout_tree::LayoutTree, getters::get_style_properties};
use crate::text3::cache::ParsedFontTrait;
use azul_core::{dom::NodeId, styled_dom::StyledDom};
use azul_css::props::basic::{pixel::PixelValue, font::{StyleFontWeight, StyleFontStyle}};

/// Resolves the computed font-size for a node by walking up the layout tree.
///
/// This function correctly implements `font-size` inheritance. If a `font-size`
/// is not explicitly set on the current node, it recursively checks its parent
/// until a value is found or it reaches the root, where it defaults to 16px.
pub fn get_resolved_font_size<T: ParsedFontTrait>(
    tree: &LayoutTree<T>,
    styled_dom: &StyledDom,
    node_index: usize,
) -> f32 {
    let mut current_idx = Some(node_index);

    while let Some(idx) = current_idx {
        let node = &tree.nodes[idx];
        if let Some(dom_id) = node.dom_node_id {
            let node_data = &styled_dom.node_data.as_container()[dom_id];
            let node_state = &styled_dom.styled_nodes.as_container()[dom_id].state;
            let cache = &styled_dom.css_property_cache.ptr;

            if let Some(size) = cache
                .get_font_size(node_data, &dom_id, node_state)
                .and_then(|v| v.get_property().cloned())
            {
                // Found an explicit font-size on this node or an ancestor.
                // TODO: Handle 'em' and 'rem' units correctly. For now, assume pixels.
                return size.inner.to_pixels(16.0); // Pass a default for % fallback.
            }
        }
        // Move to the parent node to check for inherited values.
        current_idx = node.parent;
    }

    // No font-size found in the entire ancestry, use the root default.
    16.0
}

/// Resolves the computed font-weight for a node by walking up the layout tree.
///
/// This function correctly implements `font-weight` inheritance. If a `font-weight`
/// is not explicitly set on the current node, it recursively checks its parent
/// until a value is found or it reaches the root, where it defaults to Normal (400).
pub fn get_resolved_font_weight<T: ParsedFontTrait>(
    tree: &LayoutTree<T>,
    styled_dom: &StyledDom,
    node_index: usize,
) -> StyleFontWeight {
    let mut current_idx = Some(node_index);

    while let Some(idx) = current_idx {
        let node = &tree.nodes[idx];
        if let Some(dom_id) = node.dom_node_id {
            let node_data = &styled_dom.node_data.as_container()[dom_id];
            let node_state = &styled_dom.styled_nodes.as_container()[dom_id].state;
            let cache = &styled_dom.css_property_cache.ptr;

            if let Some(weight) = cache
                .get_font_weight(node_data, &dom_id, node_state)
                .and_then(|v| v.get_property().cloned())
            {
                return weight;
            }
        }
        // Move to the parent node to check for inherited values.
        current_idx = node.parent;
    }

    // No font-weight found in the entire ancestry, use the default.
    StyleFontWeight::Normal
}

/// Resolves the computed font-style for a node by walking up the layout tree.
///
/// This function correctly implements `font-style` inheritance. If a `font-style`
/// is not explicitly set on the current node, it recursively checks its parent
/// until a value is found or it reaches the root, where it defaults to Normal.
pub fn get_resolved_font_style<T: ParsedFontTrait>(
    tree: &LayoutTree<T>,
    styled_dom: &StyledDom,
    node_index: usize,
) -> StyleFontStyle {
    let mut current_idx = Some(node_index);

    while let Some(idx) = current_idx {
        let node = &tree.nodes[idx];
        if let Some(dom_id) = node.dom_node_id {
            let node_data = &styled_dom.node_data.as_container()[dom_id];
            let node_state = &styled_dom.styled_nodes.as_container()[dom_id].state;
            let cache = &styled_dom.css_property_cache.ptr;

            if let Some(style) = cache
                .get_font_style(node_data, &dom_id, node_state)
                .and_then(|v| v.get_property().cloned())
            {
                return style;
            }
        }
        // Move to the parent node to check for inherited values.
        current_idx = node.parent;
    }

    // No font-style found in the entire ancestry, use the default.
    StyleFontStyle::Normal
}
