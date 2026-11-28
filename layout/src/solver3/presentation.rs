//! What the user's contrast and transparency settings do to what a node paints.
//!
//! The numbers are `azul_css::presentation`; this is the layout's half: which colour of which node
//! is an ink, a fill or an edge, and what surface lies behind it. The painters' getters
//! (`getters::get_used_text_color`, `get_background_contents`, `get_border_info`, `get_box_shadows`,
//! `get_backdrop_filter`) call it, so every widget, every app and every test document is covered by
//! the one place, and nothing is written per widget.
//!
//! * Reduced transparency: a frosted surface - a node with a `backdrop-filter` - paints its
//!   translucent fill opaque (over what lies behind it) and no blur. A scrim, a tint and a shadow
//!   are not frosted glass and stay as they are.
//! * Increased contrast: a text is read at 7:1 on the surface behind it, an opaque fill is one an
//!   extreme ink reaches that on, a border and a ring (a shadow without blur and offset) are seen
//!   at 4.5:1, and a gradient fill is one flat colour (a ramp that carries no text keeps its stops
//!   where the subtree says `--azul-contrast-adjust: none`: a colour picker's hue strip, a chart's
//!   series, a map).
//!
//! Both are free when no setting is on: one read of the window's preferences, no walk.

use azul_core::{
    id::NodeId,
    styled_dom::{StyledDom, StyledNodeState},
};
use azul_css::{
    dynamic_selector::resolve_system_color_ref,
    presentation::{more_contrast_edge, more_contrast_ink, more_contrast_surface, opaque_over},
    props::{
        basic::color::{ColorOrSystem, ColorU, SystemColorRef},
        style::{box_shadow::StyleBoxShadow, StyleBackgroundContent},
    },
};

/// The custom property that exempts a subtree from increased contrast (it inherits): `none` leaves
/// the colours of the subtree as the page wrote them. For a colour that IS the content - a hue
/// strip, a series of a chart, a map, a photo's frame - not for chrome.
pub const CONTRAST_ADJUST_PROPERTY: &str = "azul-contrast-adjust";

/// The display settings that change what is painted, read from the window's context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Presentation {
    /// Reduced transparency: frosted surfaces are opaque and unblurred.
    pub opaque: bool,
    /// Increased contrast (or a contrast theme): ink 7:1, edges 4.5:1, readable fills.
    pub more_contrast: bool,
}

impl Presentation {
    /// What `styled_dom`'s window asked for; nothing for a document no window has adopted.
    #[must_use]
    pub fn of(styled_dom: &StyledDom) -> Self {
        styled_dom
            .css_property_cache
            .ptr
            .dynamic_context
            .as_deref()
            .map_or_else(Self::default, |context| Self {
                opaque: context.transparency_is_reduced(),
                more_contrast: context.contrast_is_increased(),
            })
    }

    /// Any setting is on.
    #[must_use]
    pub const fn is_active(self) -> bool {
        self.opaque || self.more_contrast
    }
}

/// Whether `node` is under `--azul-contrast-adjust: none`.
fn exempt(styled_dom: &StyledDom, node: NodeId) -> bool {
    let envs = &styled_dom.css_property_cache.ptr.custom_property_envs;
    !envs.is_empty()
        && envs
            .get(
                node.index(),
                azul_css::dynamic_selector::PseudoStateType::Normal,
                CONTRAST_ADJUST_PROPERTY,
            )
            .is_some_and(|value| value.trim().eq_ignore_ascii_case("none"))
}

/// The colour of a layer as far as one colour can say it: a solid one, a gradient's mean, `None`
/// for an image.
fn layer_colour(layer: &StyleBackgroundContent) -> Option<ColorU> {
    fn mean(stops: impl Iterator<Item = ColorOrSystem>) -> Option<ColorU> {
        let (mut red, mut green, mut blue, mut alpha, mut count) = (0_u32, 0_u32, 0_u32, 0_u32, 0_u32);
        for stop in stops {
            let ColorOrSystem::Color(colour) = stop else { continue };
            red += u32::from(colour.r);
            green += u32::from(colour.g);
            blue += u32::from(colour.b);
            alpha += u32::from(colour.a);
            count += 1;
        }
        // each sum is at most 255 * the number of stops: the mean fits a byte
        let byte = |sum: u32| u8::try_from(sum / count).unwrap_or(u8::MAX);
        (count > 0).then(|| ColorU { r: byte(red), g: byte(green), b: byte(blue), a: byte(alpha) })
    }
    match layer {
        StyleBackgroundContent::Color(c) => Some(*c),
        StyleBackgroundContent::LinearGradient(g) => mean(g.stops.as_ref().iter().map(|s| s.color)),
        StyleBackgroundContent::RadialGradient(g) => mean(g.stops.as_ref().iter().map(|s| s.color)),
        StyleBackgroundContent::ConicGradient(g) => mean(g.stops.as_ref().iter().map(|s| s.color)),
        StyleBackgroundContent::Image(_) | StyleBackgroundContent::SystemColor(_) => None,
    }
}

/// The colour of the page the window paints on.
fn canvas(styled_dom: &StyledDom) -> ColorU {
    let context = styled_dom.css_property_cache.ptr.dynamic_context.as_deref();
    let mut colour = resolve_system_color_ref(SystemColorRef::WindowBackground, context);
    colour.a = u8::MAX;
    colour
}

/// What `node`'s own fill (as presented) paints over `below`.
fn paint_over(styled_dom: &StyledDom, node: NodeId, below: ColorU) -> ColorU {
    let state = &styled_dom.styled_nodes.as_container()[node].styled_node_state;
    super::getters::get_background_contents(styled_dom, node, state)
        .iter()
        .filter_map(layer_colour)
        .fold(below, |surface, colour| opaque_over(colour, surface))
}

/// The surface `node`'s own paint lies on: what its ancestors paint, from the canvas down. Opaque.
#[must_use]
pub fn surface_under(styled_dom: &StyledDom, node: NodeId) -> ColorU {
    let hierarchy = styled_dom.node_hierarchy.as_container();
    let mut ancestors = Vec::new();
    let mut at = hierarchy.get(node).and_then(azul_core::styled_dom::NodeHierarchyItem::parent_id);
    while let Some(parent) = at {
        ancestors.push(parent);
        at = hierarchy.get(parent).and_then(azul_core::styled_dom::NodeHierarchyItem::parent_id);
    }
    ancestors
        .into_iter()
        .rev()
        .fold(canvas(styled_dom), |surface, ancestor| paint_over(styled_dom, ancestor, surface))
}

/// The surface a node's CONTENT is seen on: its own fill over [`surface_under`]. Opaque.
#[must_use]
pub fn surface_at(styled_dom: &StyledDom, node: NodeId) -> ColorU {
    paint_over(styled_dom, node, surface_under(styled_dom, node))
}

/// The `color` of `node` as the user's contrast setting lets it be read.
#[must_use]
pub fn present_ink(styled_dom: &StyledDom, node: NodeId, ink: ColorU) -> ColorU {
    // a colour that paints nothing (`color: transparent`: an icon font hidden on purpose) stays so
    if !Presentation::of(styled_dom).more_contrast || ink.a == 0 || exempt(styled_dom, node) {
        return ink;
    }
    more_contrast_ink(ink, surface_at(styled_dom, node))
}

/// A border colour of `node` as the user's contrast setting lets it be seen.
#[must_use]
pub fn present_edge(styled_dom: &StyledDom, node: NodeId, edge: ColorU) -> ColorU {
    if !Presentation::of(styled_dom).more_contrast || edge.a == 0 || exempt(styled_dom, node) {
        return edge;
    }
    more_contrast_edge(edge, surface_under(styled_dom, node))
}

/// Whether a shadow is a ring (a focus halo, a border drawn as a shadow): no blur, no offset, a
/// spread.
fn is_ring(shadow: &StyleBoxShadow) -> bool {
    let px = |v: &azul_css::props::basic::pixel::PixelValueNoPercent| v.to_pixels_internal(16.0, 16.0);
    px(&shadow.blur_radius) == 0.0 && px(&shadow.offset_x) == 0.0 && px(&shadow.offset_y) == 0.0
        && px(&shadow.spread_radius) > 0.0
}

/// A shadow of `node` as the user's contrast setting lets it be seen: a RING is an edge (a focus
/// ring must be seen), a soft shadow is decoration and stays.
#[must_use]
pub fn present_shadow(styled_dom: &StyledDom, node: NodeId, mut shadow: StyleBoxShadow) -> StyleBoxShadow {
    if !Presentation::of(styled_dom).more_contrast
        || shadow.color.a == 0
        || !is_ring(&shadow)
        || exempt(styled_dom, node)
    {
        return shadow;
    }
    // an inset ring is seen on the node's own surface, an outer one on what lies behind it
    let surface = if matches!(shadow.clip_mode, azul_css::props::style::BoxShadowClipMode::Inset) {
        surface_at(styled_dom, node)
    } else {
        surface_under(styled_dom, node)
    };
    shadow.color = more_contrast_edge(shadow.color, surface);
    shadow
}

/// The background layers of `node` as the user's settings let them be painted.
///
/// `frosted`: the node has a backdrop filter (the layers are read before the filter is dropped).
#[must_use]
pub fn present_layers(
    styled_dom: &StyledDom,
    node: NodeId,
    frosted: bool,
    layers: Vec<StyleBackgroundContent>,
) -> Vec<StyleBackgroundContent> {
    let presentation = Presentation::of(styled_dom);
    if !presentation.is_active() || layers.is_empty() {
        return layers;
    }
    let adjust = presentation.more_contrast && !exempt(styled_dom, node);
    let make_opaque = presentation.opaque && frosted;
    if !adjust && !make_opaque {
        return layers;
    }
    // what lies behind, for the translucent fill of a frosted surface to become what it looks like
    let behind = make_opaque.then(|| surface_under(styled_dom, node));
    let mut out = Vec::with_capacity(layers.len());
    let mut surface = behind;
    for layer in layers {
        let flat = match &layer {
            StyleBackgroundContent::Color(c) => Some(*c),
            // a ramp is one flat colour for a text to be read on (unless the subtree is exempt, above)
            StyleBackgroundContent::LinearGradient(_)
            | StyleBackgroundContent::RadialGradient(_)
            | StyleBackgroundContent::ConicGradient(_)
                if adjust =>
            {
                layer_colour(&layer)
            }
            _ => None,
        };
        let Some(mut colour) = flat else {
            out.push(layer);
            continue;
        };
        if let Some(below) = surface.as_mut() {
            colour = opaque_over(colour, *below);
            *below = colour;
        }
        // only an opaque fill is moved: a translucent one is a tint over something else, and a
        // scrim must stay a scrim
        if adjust && colour.a == u8::MAX {
            colour = more_contrast_surface(colour);
        }
        out.push(StyleBackgroundContent::Color(colour));
    }
    out
}

/// Whether the declared backdrop filter of `node` exists: what makes a surface frosted.
#[must_use]
pub fn is_frosted(styled_dom: &StyledDom, node: NodeId, state: &StyledNodeState) -> bool {
    super::getters::get_backdrop_filter_declared(styled_dom, node, state).is_some()
}

#[cfg(test)]
#[path = "presentation_test.rs"]
mod presentation_test;
