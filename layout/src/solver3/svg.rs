//! SVG content laid out and painted through its `<svg>`'s user space.
//!
//! An SVG element keeps its own attributes on its node (the builtin
//! renderers, `azul_core::xml::element`): its geometry, its `transform`, a
//! text's `x` / `y` and `font-size` are USER UNITS of the nearest `<svg>`.
//! Layout and paint map them through that `<svg>`'s CURRENT box (its viewBox
//! onto the box it was laid out at) and the `transform`s between it and the
//! element - here, so the two agree.

use alloc::{sync::Arc, vec::Vec};

use azul_core::{
    dom::{NodeId, NodeType, SvgNodeData},
    styled_dom::{NodeHierarchyItem, StyledDom},
    svg::{parse_svg_transform, SvgAffine},
};

use crate::{
    solver3::layout_tree::{LayoutNodeId, LayoutTree},
    text3::cache::{InlineContent, LineHeight, Spacing, StyleProperties, UnifiedConstraints},
};

/// The nearest `<svg>` (a node with a viewBox) at or above `node`.
#[must_use]
pub fn svg_ancestor(styled_dom: &StyledDom, node: NodeId) -> Option<NodeId> {
    let node_data = styled_dom.node_data.as_container();
    let hierarchy = styled_dom.node_hierarchy.as_container();
    let mut cursor = Some(node);
    while let Some(id) = cursor {
        if let Some(SvgNodeData::ViewBox { .. }) =
            node_data.get(id).and_then(azul_core::dom::NodeData::get_svg_data)
        {
            return Some(id);
        }
        cursor = hierarchy.get(id).and_then(NodeHierarchyItem::parent_id);
    }
    None
}

/// The viewBox `(min_x, min_y, width, height)` of the `<svg>` `svg`.
#[must_use]
pub fn view_box_of(styled_dom: &StyledDom, svg: NodeId) -> Option<(f32, f32, f32, f32)> {
    match styled_dom
        .node_data
        .as_container()
        .get(svg)
        .and_then(azul_core::dom::NodeData::get_svg_data)
    {
        Some(SvgNodeData::ViewBox {
            min_x,
            min_y,
            width,
            height,
        }) => Some((*min_x, *min_y, *width, *height)),
        _ => None,
    }
}

/// The `transform`s between `node` and its `<svg>` composed (its own first,
/// then each group's above it - SVG 1.1 7.4: a group's transform applies to
/// everything in it): what maps `node`'s user space into its `<svg>`'s,
/// where the viewBox takes over. Identity outside an `<svg>`.
#[must_use]
pub fn user_transform(styled_dom: &StyledDom, node: NodeId) -> SvgAffine {
    let mut user = SvgAffine::IDENTITY;
    let Some(svg) = svg_ancestor(styled_dom, node) else {
        return user;
    };
    let node_data = styled_dom.node_data.as_container();
    let hierarchy = styled_dom.node_hierarchy.as_container();
    let mut cursor = Some(node);
    while let Some(id) = cursor.filter(|id| *id != svg) {
        if let Some(transform) = node_data.get(id).and_then(|n| n.get_attribute("transform")) {
            user = user.then(&parse_svg_transform(transform.as_str()));
        }
        cursor = hierarchy.get(id).and_then(NodeHierarchyItem::parent_id);
    }
    user
}

/// Whether `node` is an SVG `<text>` inside an `<svg>`.
#[must_use]
pub fn is_svg_text(styled_dom: &StyledDom, node: NodeId) -> bool {
    matches!(
        styled_dom
            .node_data
            .as_container()
            .get(node)
            .map(azul_core::dom::NodeData::get_node_type),
        Some(NodeType::SvgText)
    ) && svg_ancestor(styled_dom, node).is_some()
}

/// The first number of an SVG coordinate attribute (`x="72"`, `x="72 80"`).
fn first_number(value: &str) -> Option<f64> {
    value
        .split(|c: char| c == ',' || c.is_ascii_whitespace())
        .find(|s| !s.is_empty())?
        .parse()
        .ok()
}

/// Where the SVG text `node`'s first baseline starts, in its user units
/// (`x`, `y`; 0 when absent).
#[must_use]
pub fn text_anchor(styled_dom: &StyledDom, node: NodeId) -> (f64, f64) {
    let node_data = styled_dom.node_data.as_container();
    let coordinate = |name: &str| {
        node_data
            .get(node)
            .and_then(|n| n.get_attribute(name))
            .and_then(|v| first_number(v.as_str()))
            .unwrap_or(0.0)
    };
    (coordinate("x"), coordinate("y"))
}

/// What maps the SVG text `node`'s user units into its `<svg>`'s current
/// box (its padding box, where the `<svg>`'s absolutely positioned content
/// is placed): its transforms, then the viewBox onto the size the `<svg>`
/// was laid out at. `None` for a node that is no SVG text, or before its
/// `<svg>` has a size.
#[must_use]
pub fn text_mapping(styled_dom: &StyledDom, tree: &LayoutTree, node: NodeId) -> Option<SvgAffine> {
    if !is_svg_text(styled_dom, node) {
        return None;
    }
    let svg = svg_ancestor(styled_dom, node)?;
    let view_box = view_box_of(styled_dom, svg)?;
    let svg_index = *tree.dom_to_layout.get(&svg)?.first()?;
    let svg_node = tree.get(svg_index)?;
    let size = svg_node.used_size?;
    let border = svg_node.box_props.unpack().border;
    let (width, height) = (
        (size.width - border.left - border.right).max(0.0),
        (size.height - border.top - border.bottom).max(0.0),
    );
    if !(width > 0.0 && height > 0.0) {
        return None;
    }
    Some(user_transform(styled_dom, node).then(&SvgAffine::view_box_mapping(
        view_box, width, height,
    )))
}

/// The scale an SVG text's user-unit sizes (`font-size`, spacing, a tspan's
/// `dx`) take on the screen: its mapping's length scale. `None` for a node
/// that is no SVG text.
#[must_use]
pub fn text_scale(styled_dom: &StyledDom, tree: &LayoutTree, layout_node: usize) -> Option<f32> {
    let dom = tree.get(LayoutNodeId::new(layout_node))?.dom_node_id?;
    #[allow(clippy::cast_possible_truncation)] // a font scale
    let scale = text_mapping(styled_dom, tree, dom)?.length_scale() as f32;
    Some(if scale.is_finite() && scale > 0.0 { scale } else { 1.0 })
}

/// The `dx` (user units) a `<tspan>` puts before its first character, if
/// `text` is that tspan's first text.
fn tspan_dx(styled_dom: &StyledDom, text: NodeId) -> Option<f32> {
    let hierarchy = styled_dom.node_hierarchy.as_container();
    let tspan = hierarchy.get(text)?.parent_id()?;
    let node_data = styled_dom.node_data.as_container();
    if !matches!(node_data.get(tspan)?.get_node_type(), NodeType::SvgTspan) {
        return None;
    }
    if hierarchy.get(tspan)?.first_child_id(tspan) != Some(text) {
        return None;
    }
    #[allow(clippy::cast_possible_truncation)] // user units
    let dx = first_number(node_data.get(tspan)?.get_attribute("dx")?.as_str())? as f32;
    (dx.is_finite() && dx != 0.0).then_some(dx)
}

/// `style` with every length it carries scaled by `s` (its font size, a
/// pixel line height, pixel spacing).
fn scaled_style(style: &StyleProperties, s: f32) -> StyleProperties {
    let mut scaled = style.clone();
    scaled.font_size_px *= s;
    if let LineHeight::Px(px) = scaled.line_height {
        scaled.line_height = LineHeight::Px(px * s);
    }
    let spacing = |sp: Spacing| match sp {
        #[allow(clippy::cast_precision_loss)] // whole pixels
        Spacing::Px(px) => Spacing::PxF(px as f32 * s),
        Spacing::PxF(px) => Spacing::PxF(px * s),
        other => other,
    };
    scaled.letter_spacing = spacing(scaled.letter_spacing);
    scaled.word_spacing = spacing(scaled.word_spacing);
    scaled
}

/// An SVG text's collected inline content as it is laid out: its user-unit
/// sizes scaled by `s` (the runs' styles, the widths of measured spaces), and
/// each `<tspan dx>`'s shift - printpdf's kerning, often negative - as a
/// space of that width before the tspan's first character.
#[must_use]
pub fn svg_text_content(
    styled_dom: &StyledDom,
    content: &[InlineContent],
    s: f32,
) -> Vec<InlineContent> {
    let mut out = Vec::with_capacity(content.len());
    let mut last_source = None;
    for item in content {
        if let InlineContent::Text(run) = item {
            if run.source_node_id != last_source {
                last_source = run.source_node_id;
                if let Some(dx) = run.source_node_id.and_then(|t| tspan_dx(styled_dom, t)) {
                    out.push(InlineContent::Space(crate::text3::cache::InlineSpace {
                        width: dx * s,
                        is_breaking: false,
                        is_stretchy: false,
                    }));
                }
            }
        }
        out.push(scaled_item(item, s));
    }
    out
}

/// One inline item with its user-unit sizes scaled by `s`.
fn scaled_item(item: &InlineContent, s: f32) -> InlineContent {
    if (s - 1.0).abs() < 1e-6 {
        return item.clone();
    }
    match item {
        InlineContent::Text(run) => {
            let mut run = run.clone();
            run.style = Arc::new(scaled_style(&run.style, s));
            InlineContent::Text(run)
        }
        InlineContent::Space(space) => {
            let mut space = space.clone();
            space.width *= s;
            InlineContent::Space(space)
        }
        other => other.clone(),
    }
}

/// An SVG text's line constraints with their font-derived lengths scaled by
/// `s` (the strut, a pixel line height, the indent).
pub fn scale_constraints(constraints: &mut UnifiedConstraints, s: f32) {
    if let LineHeight::Px(px) = constraints.line_height {
        constraints.line_height = LineHeight::Px(px * s);
    }
    constraints.strut_ascent *= s;
    constraints.strut_descent *= s;
    constraints.strut_x_height *= s;
    constraints.strut_cap_height *= s;
    constraints.strut_font_size *= s;
    constraints.ch_width *= s;
    constraints.text_indent *= s;
}
