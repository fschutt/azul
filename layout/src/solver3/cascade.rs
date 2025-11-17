//! solver3/cascade.rs
//!
//! Implements CSS cascading and inheritance using the pre-computed values cache.
//!
//! The CssPropertyCache now pre-computes all inherited values and resolves relative units
//! (em, %, etc.) during StyledDom construction. This eliminates the need for tree-walking
//! during layout and provides consistent, efficient access to resolved CSS values.

use crate::solver3::{layout_tree::LayoutTree, getters::get_style_properties};
use crate::text3::cache::ParsedFontTrait;
use azul_core::{dom::NodeId, styled_dom::StyledDom};
use azul_css::props::{
    basic::{pixel::PixelValue, font::{StyleFontWeight, StyleFontStyle}},
    property::CssPropertyType,
};

/// Resolves the computed font-size for a node using the pre-computed cache.
///
/// This function retrieves the font-size from the `computed_values` cache,
/// which already contains resolved and inherited values. No tree-walking needed.
///
/// # Arguments
/// * `tree` - The layout tree (used to map node_index to dom_id)
/// * `styled_dom` - The styled DOM containing the property cache
/// * `node_index` - The layout tree node index
///
/// # Returns
/// The computed font-size in pixels, defaulting to 16.0 if not found.
pub fn get_resolved_font_size<T: ParsedFontTrait>(
    tree: &LayoutTree<T>,
    styled_dom: &StyledDom,
    node_index: usize,
) -> f32 {
    let node = &tree.nodes[node_index];
    let dom_id = match node.dom_node_id {
        Some(id) => id,
        None => return 16.0, // No DOM node, use default
    };

    let cache = &styled_dom.css_property_cache.ptr;
    
    // Check computed_values first - these are pre-resolved and inherited
    if let Some(computed) = cache.computed_values.get(&dom_id) {
        if let Some(font_size_prop) = computed.get(&CssPropertyType::FontSize) {
            if let azul_css::props::property::CssProperty::FontSize(val) = &font_size_prop.property {
                if let Some(size) = val.get_property() {
                    // The value is already resolved to pixels in computed_values
                    return size.inner.to_pixels(16.0);
                }
            }
        }
    }

    // Fallback: check the cache's getter (for non-inherited or explicitly set values)
    let node_data = &styled_dom.node_data.as_container()[dom_id];
    let node_state = &styled_dom.styled_nodes.as_container()[dom_id].state;
    
    cache
        .get_font_size(node_data, &dom_id, node_state)
        .and_then(|v| v.get_property().cloned())
        .map(|size| size.inner.to_pixels(16.0))
        .unwrap_or(16.0)
}

/// Resolves the computed font-weight for a node using the pre-computed cache.
///
/// This function retrieves the font-weight from the `computed_values` cache,
/// which already contains inherited values. No tree-walking needed.
///
/// # Arguments
/// * `tree` - The layout tree (used to map node_index to dom_id)
/// * `styled_dom` - The styled DOM containing the property cache
/// * `node_index` - The layout tree node index
///
/// # Returns
/// The computed font-weight, defaulting to Normal (400) if not found.
pub fn get_resolved_font_weight<T: ParsedFontTrait>(
    tree: &LayoutTree<T>,
    styled_dom: &StyledDom,
    node_index: usize,
) -> StyleFontWeight {
    let node = &tree.nodes[node_index];
    let dom_id = match node.dom_node_id {
        Some(id) => id,
        None => return StyleFontWeight::Normal,
    };

    let cache = &styled_dom.css_property_cache.ptr;
    
    // Check computed_values first - these contain inherited values
    if let Some(computed) = cache.computed_values.get(&dom_id) {
        if let Some(font_weight_prop) = computed.get(&CssPropertyType::FontWeight) {
            if let azul_css::props::property::CssProperty::FontWeight(val) = &font_weight_prop.property {
                if let Some(weight) = val.get_property() {
                    return *weight;
                }
            }
        }
    }

    // Fallback: check the cache's getter
    let node_data = &styled_dom.node_data.as_container()[dom_id];
    let node_state = &styled_dom.styled_nodes.as_container()[dom_id].state;
    
    cache
        .get_font_weight(node_data, &dom_id, node_state)
        .and_then(|v| v.get_property().cloned())
        .unwrap_or(StyleFontWeight::Normal)
}

/// Resolves the computed font-style for a node using the pre-computed cache.
///
/// This function retrieves the font-style from the `computed_values` cache,
/// which already contains inherited values. No tree-walking needed.
///
/// # Arguments
/// * `tree` - The layout tree (used to map node_index to dom_id)
/// * `styled_dom` - The styled DOM containing the property cache
/// * `node_index` - The layout tree node index
///
/// # Returns
/// The computed font-style, defaulting to Normal if not found.
pub fn get_resolved_font_style<T: ParsedFontTrait>(
    tree: &LayoutTree<T>,
    styled_dom: &StyledDom,
    node_index: usize,
) -> StyleFontStyle {
    let node = &tree.nodes[node_index];
    let dom_id = match node.dom_node_id {
        Some(id) => id,
        None => return StyleFontStyle::Normal,
    };

    let cache = &styled_dom.css_property_cache.ptr;
    
    // Check computed_values first - these contain inherited values
    if let Some(computed) = cache.computed_values.get(&dom_id) {
        if let Some(font_style_prop) = computed.get(&CssPropertyType::FontStyle) {
            if let azul_css::props::property::CssProperty::FontStyle(val) = &font_style_prop.property {
                if let Some(style) = val.get_property() {
                    return *style;
                }
            }
        }
    }

    // Fallback: check the cache's getter
    let node_data = &styled_dom.node_data.as_container()[dom_id];
    let node_state = &styled_dom.styled_nodes.as_container()[dom_id].state;
    
    cache
        .get_font_style(node_data, &dom_id, node_state)
        .and_then(|v| v.get_property().cloned())
        .unwrap_or(StyleFontStyle::Normal)
}
